//! INSPECTOR-DUMP — a headed framebuffer KAT for the GTK Dear ImGui terminal
//! inspector. Models the `gtk_glyph_dump` harness: it constructs a headed GTK
//! surface, opens the inspector window (`control_inspector(Toggle)`), pumps the
//! main loop until the `InspectorWidget` realizes + renders its ImGui panels,
//! reads GDK's GLArea framebuffer back and dumps it to a PPM (path from the
//! `RUSTTY_DUMP_PPM` env var).
//!
//! Beyond the glyph-dump's single readback, this KAT proves LIVE INPUT reaches
//! the inspector: it focuses the inspector's "Keyboard" panel (the docked window
//! that renders the recorded key-event stream), captures a baseline frame A,
//! feeds a synthetic key event through the core surface's `keyCallback` (which
//! records it into the inspector via the `InspEvGuard` drop, exactly as a real
//! keypress would), renders again, and captures frame B. It then asserts:
//!
//!   (a) the inspector framebuffer is NOT a uniform clear (the `#282C34` clear
//!       color) — the ImGui panels actually drew; and
//!   (b) a needle-scan of the keyboard panel shows a pixel delta between A and B
//!       — the synthetic key event reached the live panel.
//!
//! Run:  DISPLAY=:0 XAUTHORITY=/run/user/1000/gdm/Xauthority \
//!         RUSTTY_DUMP_PPM=/path/out.ppm \
//!         cargo test -p rustty-apprt --features gtk --test gtk_inspector_dump \
//!         -- --ignored --exact inspector_dump
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")` — compiles out off the gate.
#![cfg(all(target_os = "linux", feature = "gtk"))]
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

/// The `#282C34` OpenGL clear the ImGui widget issues before drawing
/// (`imgui_widget.rs` `render`, `imgui_widget.zig:318-320`).
const CLEAR_RGB: [u8; 3] = [0x28, 0x2C, 0x34];

/// Recursively find the first [`gtk4::GLArea`] under `widget`.
fn find_gl_area(widget: &gtk4::Widget) -> Option<gtk4::GLArea> {
    if let Some(area) = widget.downcast_ref::<gtk4::GLArea>() {
        return Some(area.clone());
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        if let Some(found) = find_gl_area(&c) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

/// Find the live [`InspectorWindow`] among the process's toplevel windows (the
/// inspector is a separate top-level; `control_inspector` presents it but does
/// not return it, and `Surface::inspector` is `pub(crate)`).
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
    match unsafe { &(*core).io.backend } {
        rustty_termio::Backend::Exec(exec) => {
            exec.subprocess.pty.as_ref().expect("pty started").master
        }
    }
}

/// Make the inspector GLArea current, bind its framebuffer, and read it back to a
/// tight `w*h*4` RGBA buffer (bottom-up, as `glReadPixels` returns it). Reuses the
/// renderer crate's `read_bound_framebuffer_rgba` (which dispatches only through
/// the process-global GL loader, so it reads whichever context is current — here
/// the inspector's, NOT the terminal's).
fn capture_inspector(gl_area: &gtk4::GLArea, api: &rustty_renderer::opengl::OpenGL, w: usize, h: usize) -> Option<Vec<u8>> {
    gl_area.make_current();
    if gl_area.error().is_some() {
        return None;
    }
    gl_area.attach_buffers();
    Some(api.read_bound_framebuffer_rgba(w, h))
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
        if pa[0].abs_diff(pb[0]) > tol
            || pa[1].abs_diff(pb[1]) > tol
            || pa[2].abs_diff(pb[2]) > tol
        {
            n += 1;
        }
    }
    n
}

/// Fraction of pixels within `tol` of the `#282C34` clear color.
fn clear_fraction(rgba: &[u8], tol: u8) -> f32 {
    if rgba.is_empty() {
        return 1.0;
    }
    let mut clear = 0usize;
    let total = rgba.len() / 4;
    for px in rgba.chunks_exact(4) {
        if px[0].abs_diff(CLEAR_RGB[0]) <= tol
            && px[1].abs_diff(CLEAR_RGB[1]) <= tol
            && px[2].abs_diff(CLEAR_RGB[2]) <= tol
        {
            clear += 1;
        }
    }
    clear as f32 / total as f32
}

/// Write the captured RGBA (bottom-up) to a top-down binary PPM (P6, RGB). Same
/// helper the glyph-dump uses (`gtk_glyph_dump.rs`).
fn write_ppm(path: &str, rgba: &[u8], w: usize, h: usize) {
    let mut buf = Vec::with_capacity(w * h * 3 + 32);
    buf.extend_from_slice(format!("P6\n{w} {h}\n255\n").as_bytes());
    // glReadPixels returns rows bottom-to-top; flip to top-down for a normal image.
    for y in (0..h).rev() {
        let row = &rgba[y * w * 4..(y + 1) * w * 4];
        for x in 0..w {
            buf.push(row[x * 4]);
            buf.push(row[x * 4 + 1]);
            buf.push(row[x * 4 + 2]);
        }
    }
    std::fs::write(path, buf).expect("write PPM dump");
}

#[test]
#[ignore = "headed GTK + GL 4.3-core context + a live inspector; run with \
            DISPLAY=:0 XAUTHORITY=/run/user/1000/gdm/Xauthority \
            RUSTTY_DUMP_PPM=/path/out.ppm cargo test -p rustty-apprt --features gtk \
            --test gtk_inspector_dump -- --ignored --exact inspector_dump"]
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
        working_directory: None,
        title: None,
    });

    let window = libadwaita::ApplicationWindow::new(&application);
    window.set_default_size(800, 480);
    window.set_content(Some(&surface));
    window.present();

    let ctx = glib::MainContext::default();
    let keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);

    // Pump the main-surface GLArea so the lazy core surface gets built on its
    // first resize (same lazy-realize path the glyph-dump relies on).
    let main_gl_area = find_gl_area(surface.upcast_ref::<gtk4::Widget>())
        .expect("the surface template contains a GLArea");
    let main_pump = {
        let weak = main_gl_area.downgrade();
        glib::timeout_add_local(Duration::from_millis(30), move || match weak.upgrade() {
            Some(area) => {
                area.queue_render();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        })
    };

    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
        ctx.iteration(true);
        assert!(Instant::now() < deadline, "core surface was never built");
    }
    let core = surface.core().expect("core surface built on first GLArea resize");

    // The terminal renderer's OpenGL API — reused only for its context-agnostic
    // `read_bound_framebuffer_rgba` (it reads whichever GL context is current).
    let api: &rustty_renderer::opengl::OpenGL = unsafe { &(*core).renderer.0.api };

    // ---- Open the inspector (control_inspector Toggle) + present its window. ----
    assert!(
        surface.control_inspector(Inspector::Toggle),
        "control_inspector(Toggle) returns true"
    );

    // Pump until the inspector window + its GLArea appear.
    let inspector_deadline = Instant::now() + Duration::from_secs(20);
    let inspector_window = loop {
        ctx.iteration(true);
        if let Some(win) = find_inspector_window() {
            break win;
        }
        assert!(
            Instant::now() < inspector_deadline,
            "the inspector window never opened"
        );
    };
    let inspector_gl_area = loop {
        ctx.iteration(true);
        if let Some(area) = find_gl_area(inspector_window.upcast_ref::<gtk4::Widget>()) {
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

    // Helper: pump `iters` main-loop iterations then read the inspector back.
    let pump_and_capture = |iters: usize| -> Option<Vec<u8>> {
        let scale = inspector_gl_area.scale_factor().max(1);
        let w = (inspector_gl_area.width() * scale).max(0) as usize;
        let h = (inspector_gl_area.height() * scale).max(0) as usize;
        inspector_gl_area.queue_render();
        for _ in 0..iters {
            ctx.iteration(true);
        }
        if w == 0 || h == 0 {
            return None;
        }
        capture_inspector(&inspector_gl_area, api, w, h)
    };

    // ---- Warm-up: pump until the panels actually draw (a non-uniform frame). ----
    let warm_deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(px) = pump_and_capture(8) {
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
            a = pump_and_capture(10);
            assert!(Instant::now() < a_deadline, "frame A capture timed out");
        }
        a.expect("captured baseline inspector frame A")
    };
    let (w, h) = {
        let scale = inspector_gl_area.scale_factor().max(1);
        (
            (inspector_gl_area.width() * scale) as usize,
            (inspector_gl_area.height() * scale) as usize,
        )
    };
    assert!(w > 0 && h > 0, "the inspector GLArea viewport was sized");
    assert_eq!(frame_a.len(), w * h * 4, "frame A is a tight RGBA buffer");

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
    let _ = unsafe { (*core).keyCallback(ev) };

    // Also flush the pty so the shell's echo does not race the capture windows
    // (belt-and-braces; the key stream entry alone drives the assert).
    {
        let master = unsafe { pty_master(core) };
        use std::io::Write as _;
        use std::os::fd::FromRawFd as _;
        let mut pipe =
            std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(master) });
        let _ = pipe.flush();
    }

    // ---- Render again + capture frame B (Keyboard panel now shows the event). ----
    let frame_b = {
        let mut b = None;
        let b_deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..6 {
            b = pump_and_capture(10);
            assert!(Instant::now() < b_deadline, "frame B capture timed out");
        }
        b.expect("captured post-input inspector frame B")
    };
    assert_eq!(frame_b.len(), w * h * 4, "frame B is a tight RGBA buffer");

    // Leave the post-input frame on disk for human review.
    write_ppm(&ppm_path, &frame_b, w, h);

    // ---- ASSERT (a): the framebuffer is NOT a uniform #282C34 clear. ----
    let clear_frac = clear_fraction(&frame_b, 8);
    let drew = non_uniform_pixels(&frame_b, 8);
    assert!(
        clear_frac < 0.98 && drew > 1000,
        "inspector framebuffer looks like a uniform clear (clear_fraction={clear_frac:.3}, \
         non_uniform_pixels={drew}); the ImGui panels did not draw"
    );

    // ---- ASSERT (b): a needle-scan of the keyboard panel shows a pixel delta
    //      between A and B — the synthetic key event reached the live panel. ----
    let delta = changed_pixels(&frame_a, &frame_b, 8);
    assert!(
        delta > 50,
        "no pixel delta between the pre-key frame A and the post-key frame B \
         (changed_pixels={delta}); live input did not reach the keyboard panel"
    );

    eprintln!(
        "INSPECTOR-DUMP: wrote {ppm_path} ({w}x{h}); non_uniform={drew} clear_fraction={clear_frac:.3} \
         keyboard-panel delta A->B = {delta} px"
    );

    // Clean teardown (mirrors the glyph-dump).
    keepalive.remove();
    main_pump.remove();
    inspector_pump.remove();
    inspector_window.upcast_ref::<gtk4::Window>().destroy();
    drop(inspector_gl_area);
    drop(main_gl_area);
    {
        let app_weak = application.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(150), move || {
            if let Some(app) = app_weak.upgrade() {
                app.quit_now();
            }
        });
    }
    let run_result = app.run();
    assert!(run_result.is_ok(), "App::run returns Ok: {:?}", run_result.err());
}
