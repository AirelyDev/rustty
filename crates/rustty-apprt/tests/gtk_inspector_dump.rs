//! INSPECTOR-DUMP — a headed KAT for the GTK Dear ImGui terminal inspector, and
//! (P7-W7 T2) the two-surfaces-plus-inspector session. It constructs TWO headed GTK
//! surfaces in two windows (two render threads, each with its own EGL context and
//! its own GL function table, `opengl::gl::glad`), waits until both display frames
//! their renderers exported, then opens the inspector window
//! (`control_inspector(Toggle)`) — a GtkGLArea on GDK's context, drawn on the main
//! thread — and pumps the main loop until the `InspectorWidget` realizes + renders
//! its ImGui panels. It renders the inspector widget to a texture through its
//! window's GSK renderer and dumps it to a PPM (path from `RUSTTY_DUMP_PPM`).
//!
//! It proves LIVE INPUT reaches the inspector: it focuses the inspector's
//! "Keyboard" panel (the docked window that renders the recorded key-event stream),
//! captures a baseline frame A, feeds a synthetic key event through the core
//! surface's `keyCallback` (which records it into the inspector via the
//! `InspEvGuard` drop, exactly as a real keypress would), renders again, and
//! captures frame B. It then asserts:
//!
//!   (a) the inspector frame is not uniform — the ImGui panels actually drew;
//!   (b) a needle-scan of the keyboard panel shows a pixel delta between A and B
//!       — the synthetic key event reached the live panel; and
//!   (c) both terminals kept presenting new frames while the inspector drew — the
//!       per-thread GL tables of the two render threads and GDK's main-thread
//!       context did not clash.
//!
//! Run:  LIBGL_ALWAYS_SOFTWARE=1 RUSTTY_DUMP_PPM=/path/out.ppm \
//!         cargo test -p rustty-apprt --features gtk --test gtk_inspector_dump \
//!         -- --ignored --exact inspector_dump
//!
//! On a host without a desktop session (rvm), use the CANONICAL recipe, a
//! headless sway that TILES:
//!   .port/tools/gtk_live_headless_sway.sh cargo test -p rustty-apprt \
//!       --features gtk --test gtk_inspector_dump -- --ignored --exact inspector_dump
//! Do not float the windows (the v20b gate's `floating enable` rule). The
//! floating inspector window then covers both terminals. A covered window gets
//! no frame callbacks, so assertion (c) fails every time (P7-W7 T6).
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")` — compiles out off the gate.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
#![allow(non_snake_case)]

use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;
use gio::prelude::*;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::imgui_widget::ImguiWidget;
use rustty_apprt::gtk::class::inspector_window::InspectorWindow;
use rustty_apprt::gtk::class::surface::{CoreSurface, Surface, SurfaceOverrides};
use rustty_apprt::gtk::{self, register_resources};
use rustty_apprt_api::action::Inspector;
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;
use rustty_vt::input::key::{Action, Key, KeyEvent};
use rustty_vt::input::key_mods::Mods;

#[path = "common/gtk_live.rs"]
mod gtk_live;

fn find_inspector_window() -> Option<InspectorWindow> {
    let toplevels = gtk4::Window::toplevels();
    for i in 0..toplevels.n_items() {
        if let Some(obj) = toplevels.item(i) {
            if let Ok(win) = obj.downcast::<InspectorWindow>() {
                return Some(win);
            }
        }
    }
    None
}

unsafe fn pty_master(core: *mut CoreSurface) -> std::os::fd::RawFd {
    match unsafe { &(*(*core).io.as_ptr()).backend } {
        rustty_termio::Backend::Exec(exec) => {
            exec.subprocess.pty.as_ref().expect("pty started").master
        }
    }
}

/// Count pixels whose RGB differs from `pixel[0]` beyond `tol` in any channel —
/// a uniform clear yields 0, a frame with drawn panels/text yields many.
fn non_uniform_pixels(rgba: &[u8], tol: u8) -> usize {
    if rgba.len() < 4 {
        return 0;
    }
    let base = [rgba[0], rgba[1], rgba[2]];
    let mut n = 0;
    for px in rgba.chunks_exact(4) {
        if px[0].abs_diff(base[0]) > tol
            || px[1].abs_diff(base[1]) > tol
            || px[2].abs_diff(base[2]) > tol
        {
            n += 1;
        }
    }
    n
}

/// Count pixels differing between two same-sized frames beyond `tol` in any
/// channel — the needle-scan for "live input reached the panel".
fn changed_pixels(a: &[u8], b: &[u8], tol: u8) -> usize {
    let mut n = 0;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        if pa[0].abs_diff(pb[0]) > tol || pa[1].abs_diff(pb[1]) > tol || pa[2].abs_diff(pb[2]) > tol
        {
            n += 1;
        }
    }
    n
}

#[test]
#[ignore = "headed GTK + GL 4.3-core context + a live inspector; run with \
            LIBGL_ALWAYS_SOFTWARE=1 RUSTTY_DUMP_PPM=/path/out.ppm cargo test \
            -p rustty-apprt --features gtk --test gtk_inspector_dump -- --ignored \
            --exact inspector_dump"]
fn inspector_dump() {
    let ppm_path = std::env::var("RUSTTY_DUMP_PPM")
        .unwrap_or_else(|_| "/tmp/rustty-inspector-dump.ppm".to_string());

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

    let surface = Surface::new(SurfaceOverrides {
        command: Some(Command::Direct(vec![b"/bin/sh".to_vec()])),
        shell_integration: None,
        working_directory: None,
        title: None,
    });

    let window = libadwaita::ApplicationWindow::new(&application);
    window.set_default_size(800, 480);
    window.set_content(Some(&surface));
    window.present();

    // A second surface in a second window: a second render thread with its own
    // EGL context and GL function table.
    let surface2 = Surface::new(SurfaceOverrides {
        command: Some(Command::Direct(vec![b"/bin/sh".to_vec()])),
        shell_integration: None,
        working_directory: None,
        title: None,
    });
    let window2 = libadwaita::ApplicationWindow::new(&application);
    window2.set_default_size(640, 400);
    window2.set_content(Some(&surface2));
    window2.present();

    // Stops both render threads when it drops, including when an assertion below
    // panics. It is declared after `core_app` and `app`, so unwinding drops it
    // first, while the core app is still alive (P7-W7 T6).
    let render_threads = gtk_live::RenderThreadGuard::new(core_ptr, [&surface, &surface2]);

    let ctx = glib::MainContext::default();
    let keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);

    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() || surface2.core().is_none() {
        gtk_live::iterate(&ctx, core_ptr, &app);
        assert!(Instant::now() < deadline, "a core surface was never built");
    }
    let core = surface
        .core()
        .expect("core surface built on first RenderSurface resize");
    // SAFETY: both are live core surfaces whose shells were started by init.
    let shell_pids = unsafe {
        (
            gtk_live::shell_pid(core),
            gtk_live::shell_pid(surface2.core().expect("surface 2's core surface")),
        )
    };

    let render_surfaces = [
        gtk_live::find_render_surface(surface.upcast_ref::<gtk4::Widget>())
            .expect("surface 1 contains a RenderSurface"),
        gtk_live::find_render_surface(surface2.upcast_ref::<gtk4::Widget>())
            .expect("surface 2 contains a RenderSurface"),
    ];
    // Both terminals display a frame their renderer exported.
    let frames_deadline = Instant::now() + Duration::from_secs(20);
    while render_surfaces.iter().any(|rs| rs.texture().is_none()) {
        gtk_live::iterate(&ctx, core_ptr, &app);
        assert!(
            Instant::now() < frames_deadline,
            "a terminal never displayed a frame (two render threads)"
        );
    }

    // ---- Open the inspector (control_inspector Toggle) + present its window. ----
    assert!(
        surface.control_inspector(Inspector::Toggle),
        "control_inspector(Toggle) returns true"
    );

    // Pump until the inspector window + its GLArea (GDK's context) appear.
    let inspector_deadline = Instant::now() + Duration::from_secs(20);
    let inspector_window = loop {
        gtk_live::iterate(&ctx, core_ptr, &app);
        if let Some(win) = find_inspector_window() {
            break win;
        }
        assert!(
            Instant::now() < inspector_deadline,
            "the inspector window never opened"
        );
    };
    let inspector_gl_area = loop {
        gtk_live::iterate(&ctx, core_ptr, &app);
        if let Some(area) =
            gtk_live::find_widget::<gtk4::GLArea>(inspector_window.upcast_ref::<gtk4::Widget>())
        {
            break area;
        }
        assert!(
            Instant::now() < inspector_deadline,
            "the inspector window never grew a GLArea"
        );
    };
    // The inspector GLArea's ancestor ImguiWidget owns the ImGui context; we use
    // it to make the context current before poking `SetWindowFocusStr`.
    let imgui_widget = inspector_gl_area
        .ancestor(ImguiWidget::static_type())
        .and_then(|w| w.downcast::<ImguiWidget>().ok())
        .expect("the inspector GLArea sits inside an ImguiWidget");

    // Drive the inspector GLArea so the ImguiWidget realizes + renders.
    let inspector_pump = {
        let weak = inspector_gl_area.downgrade();
        glib::timeout_add_local(Duration::from_millis(30), move || match weak.upgrade() {
            Some(area) => {
                area.queue_render();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        })
    };

    // Helper: pump `iters` main-loop iterations then render the inspector widget
    // to a texture through its window's GSK renderer (top row first).
    let pump_and_capture = |iters: usize| -> Option<(usize, usize, Vec<u8>)> {
        inspector_gl_area.queue_render();
        for _ in 0..iters {
            gtk_live::iterate(&ctx, core_ptr, &app);
        }
        gtk_live::render_widget_rgba(inspector_gl_area.upcast_ref::<gtk4::Widget>())
    };

    // ---- Warm-up: pump until the panels actually draw (a non-uniform frame). ----
    let warm_deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some((_, _, px)) = pump_and_capture(8) {
            if non_uniform_pixels(&px, 8) > 1000 {
                break;
            }
        }
        assert!(
            Instant::now() < warm_deadline,
            "the inspector never rendered a non-uniform frame (panels did not draw)"
        );
    }

    // ---- Focus the docked "Keyboard" panel so the recorded key stream is the
    //      VISIBLE tab (windows dock as tabs into one node; only the active tab's
    //      content renders). `SetWindowFocusStr` requires the ig context current. ----
    imgui_widget
        .set_current_context()
        .expect("the realized ImguiWidget has a current ImGui context");
    unsafe { dcimgui_sys::ImGui_SetWindowFocusStr(c"Keyboard".as_ptr()) };

    // Let the focus change + layout settle, then capture baseline frame A.
    let frame_a = {
        let mut a = None;
        let a_deadline = Instant::now() + Duration::from_secs(10);
        // Pump generously so both passes settle before we snapshot.
        for _ in 0..6 {
            a = pump_and_capture(10).or(a);
            assert!(Instant::now() < a_deadline, "frame A capture timed out");
        }
        a.expect("captured baseline inspector frame A")
    };
    let (w, h, frame_a) = frame_a;
    assert!(w > 0 && h > 0, "the inspector widget was sized");
    assert_eq!(frame_a.len(), w * h * 4, "frame A is a tight RGBA buffer");

    // The terminals' current textures, to prove they keep presenting (c).
    let textures_before: Vec<_> = render_surfaces.iter().map(|rs| rs.texture()).collect();

    // ---- Feed a synthetic key event through the core surface's keyCallback.
    //      Because the inspector is active, keyCallback's InspEvGuard records the
    //      event into the inspector's key stream (exactly like a real keypress),
    //      and the "Keyboard" panel gains a visible entry. ----
    let ev = KeyEvent {
        action: Action::Press,
        key: Key::KeyA,
        mods: Mods::default(),
        consumed_mods: Mods::default(),
        composing: false,
        utf8: b"a".to_vec(),
        unshifted_codepoint: u32::from('a'),
    };
    // SAFETY: `core` is the widget's boxed, initialized core surface; called on
    // the main thread, mirroring the apprt key controller.
    let _ = unsafe { CoreSurface::keyCallback(core, ev.clone()) };

    // Also flush the pty so the shell's echo does not race the capture windows
    // (belt-and-braces; the key stream entry alone drives the assert).
    {
        let master = unsafe { pty_master(core) };
        use std::io::Write as _;
        use std::os::fd::FromRawFd as _;
        let mut pipe = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(master) });
        let _ = pipe.flush();
    }

    // ---- Render again + capture frame B (Keyboard panel now shows the event). ----
    let frame_b = {
        let mut b = None;
        let b_deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..6 {
            b = pump_and_capture(10).or(b);
            assert!(Instant::now() < b_deadline, "frame B capture timed out");
        }
        b.expect("captured post-input inspector frame B")
    };
    let (wb, hb, frame_b) = frame_b;
    assert_eq!((wb, hb), (w, h), "frame B has frame A's size");
    assert_eq!(frame_b.len(), w * h * 4, "frame B is a tight RGBA buffer");

    // Leave the post-input frame on disk for human review.
    gtk_live::write_ppm(&ppm_path, &frame_b, w, h);

    // ---- ASSERT (a): the frame is not uniform (`0fca3d34a` dropped the
    //      `#282C34` clear this used to measure against). ----
    let drew = non_uniform_pixels(&frame_b, 8);
    assert!(
        drew > 1000,
        "inspector frame looks uniform (non_uniform_pixels={drew}); the ImGui panels \
         did not draw"
    );

    // ---- ASSERT (b): a needle-scan of the keyboard panel shows a pixel delta
    //      between A and B — the synthetic key event reached the live panel. ----
    let delta = changed_pixels(&frame_a, &frame_b, 8);
    assert!(
        delta > 50,
        "no pixel delta between the pre-key frame A and the post-key frame B \
         (changed_pixels={delta}); live input did not reach the keyboard panel"
    );

    // ---- ASSERT (c): both terminals presented new frames while the inspector
    //      drew on the main thread (two render threads + GDK's context). ----
    let present_deadline = Instant::now() + Duration::from_secs(10);
    let mut both_new = false;
    while Instant::now() < present_deadline {
        // Nudge both terminals to draw (a key through the second one too).
        gtk_live::iterate(&ctx, core_ptr, &app);
        both_new = render_surfaces
            .iter()
            .zip(&textures_before)
            .all(|(rs, before)| rs.texture().is_some() && rs.texture() != *before);
        if both_new {
            break;
        }
        for c in [surface.core(), surface2.core()].into_iter().flatten() {
            // SAFETY: the widget's boxed, initialized core surface, main thread.
            let _ = unsafe { CoreSurface::keyCallback(c, ev.clone()) };
        }
    }
    // A terminal's texture changes only when GTK snapshots its window, and a
    // compositor sends no frame callbacks to a window it has fully covered. The
    // floating inspector window covers both terminals under a floating
    // compositor, so this assertion needs the tiling recipe in the header.
    assert!(
        both_new,
        "both terminals kept presenting frames alongside the inspector (new texture \
         per terminal: {:?}; is a window covering the terminals? run under the \
         tiling headless-sway recipe in this file's header)",
        render_surfaces
            .iter()
            .zip(&textures_before)
            .map(|(rs, before)| rs.texture().is_some() && rs.texture() != *before)
            .collect::<Vec<_>>()
    );

    eprintln!(
        "INSPECTOR-DUMP: wrote {ppm_path} ({w}x{h}); non_uniform={drew} \
         keyboard-panel delta A->B = {delta} px; two terminals presenting"
    );

    // Clean teardown (mirrors the glyph-dump).
    keepalive.remove();
    inspector_pump.remove();
    inspector_window.upcast_ref::<gtk4::Window>().destroy();
    drop(inspector_gl_area);
    drop(render_surfaces);
    {
        let app_weak = application.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(150), move || {
            if let Some(app) = app_weak.upgrade() {
                app.quit_now();
            }
        });
    }
    // Drain the core-app mailbox before re-entering the run loop: nothing drains it
    // during the manual `ctx.iteration` pump (the core-app `tick` is only wired
    // into `app.run()`), and `app.run()` emits `activate`, whose `new_window_thunk`
    // does a BLOCKING `push(NewWindow, Timeout::Forever)` that a full queue would
    // deadlock.
    while core_app.mailbox.pop().is_some() {}

    let run_result = app.run();
    assert!(
        run_result.is_ok(),
        "App::run returns Ok: {:?}",
        run_result.err()
    );

    // Close both terminals the way the app does (each window destroyed, each
    // `Surface` finalized, threads joined, shell reaped), then tear the apps down
    // in `run_gtk`'s order (P7-W7 T6).
    drop(imgui_widget);
    gtk_live::close_surface(&ctx, window, surface, shell_pids.0);
    gtk_live::close_surface(&ctx, window2, surface2, shell_pids.1);
    gtk_live::close_other_windows(&ctx);
    drop(render_threads);
    app.terminate();
    core_app.destroy();
}
