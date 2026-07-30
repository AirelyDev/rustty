//! L5.3 WINDOW-TAB-SURFACE WIRE — the WINDOW-LIVE KAT: a LIVE TERMINAL inside the
//! REAL `GhosttyWindow` chrome, end to end.
//!
//! Where `gtk_first_light.rs` proved the `class::Surface` widget renders a live
//! terminal in a PLAIN `adw::ApplicationWindow` (test scaffolding), THIS KAT proves
//! the real product path: `Window::new` + `Window::new_tab_for_window(None, …)` —
//! the same calls `performAction .new_window` makes (`application.zig:2253-2331`) —
//! host a [`Tab`] → `SplitTree` → `SurfaceScrolledWindow` → [`Surface`] chain
//! inside the window's `adw::TabView`, and that chain lazily stands up the real
//! core surface, draws a real frame, and runs a real `/bin/sh`.
//!
//! WHAT THIS PROVES (the task brief's body):
//!   (a) after `new_tab_for_window`, the window's `TabView` holds ONE `TabPage`
//!       whose child is a [`Tab`] hosting a live [`Surface`]
//!       (`window.zig:419-507` `newTabPage`).
//!   (b) `Surface::core()` becomes `Some` — the lazy core surface built on the
//!       GLArea's first resize (invariant 3), reached THROUGH the window chain.
//!   (c) a real frame draws: `drawFrame(sync=true)` returns `Ok` with GDK's GLArea
//!       context current + its framebuffer bound, and a `glReadPixels` readback of
//!       that framebuffer is not uniformly zero (the render contract runs).
//!   (d) the shell is LIVE — `echo rustty-window-live\n` written into the pty
//!       round-trips through the io thread into the core `Screen`.
//!
//! The draw/readback discipline (make_current + attach_buffers + drawFrame + read
//! the SAME bound framebuffer) and the run notes are `gtk_first_light.rs`'s — see
//! its module doc for the full rationale. Run with:
//!   LIBGL_ALWAYS_SOFTWARE=1 [xvfb-run -a] \
//!     cargo test -p rustty-apprt --features gtk --test gtk_window_live -- --ignored
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")` — off the gate the whole
//! file compiles out.
#![cfg(all(target_os = "linux", feature = "gtk"))]
// PARITY: the renderer-seam entry points are camelCase (`drawFrame`), matching the
// reference `Renderer` API the port preserves verbatim (R1/R2).
#![allow(non_snake_case)]

use std::io::Write as _;
use std::os::fd::{FromRawFd as _, RawFd};
use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;
use gio::prelude::*;
use gtk4::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::surface::{CoreSurface, Surface};
use rustty_apprt::gtk::class::tab::{Tab, TabOverrides};
use rustty_apprt::gtk::class::window::Window;
use rustty_apprt::gtk::{self, register_resources};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;

/// The marker the live `/bin/sh` echoes back through the pty into the `Screen`.
const NEEDLE: &[u8] = b"rustty-window-live";

/// Recursively find the terminal `GLArea` inside a widget tree (the first-light
/// KAT's helper: the GLArea is a private template child with no public accessor).
fn find_gl_area(widget: &gtk4::Widget) -> Option<gtk4::GLArea> {
    if let Ok(area) = widget.clone().downcast::<gtk4::GLArea>() {
        return Some(area);
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

/// Dump the live core surface's ACTIVE `Screen` text UNDER THE RENDERER MUTEX
/// (the `live_shell.rs`/first-light determinism discipline).
///
/// # Safety
/// `core` must point at a live, initialized [`CoreSurface`]; called on the main
/// thread while the io thread is the only other toucher (under the mutex).
unsafe fn screen_dump(core: *mut CoreSurface) -> Vec<u8> {
    unsafe {
        let mutex = (*core).renderer_state.mutex;
        (*mutex).lock();
        let active = (*(*core).renderer_state.terminal).screens.active;
        let dump = (*active).dump_string_alloc(
            Global,
            rustty_vt::point::Point::Active(rustty_vt::point::Coordinate::default()),
        );
        (*mutex).unlock();
        dump.unwrap_or_default()
    }
}

/// The pty master fd of the live surface's `/bin/sh` (the parent side).
///
/// # Safety
/// `core` must point at a live, initialized [`CoreSurface`] whose backend is a
/// started `Exec`.
unsafe fn pty_master(core: *mut CoreSurface) -> RawFd {
    match unsafe { &(*core).io.backend } {
        rustty_termio::Backend::Exec(exec) => {
            exec.subprocess.pty.as_ref().expect("pty started").master
        }
    }
}

/// THE WINDOW-LIVE KAT. See the module doc for the full body/reference map.
#[test]
#[ignore = "headed GTK + a GL 4.3-core context + a live shell; run via \
            LIBGL_ALWAYS_SOFTWARE=1 [xvfb-run -a] cargo test -p rustty-apprt \
            --features gtk --test gtk_window_live -- --ignored"]
fn window_hosts_a_live_terminal() {
    // ---- 0-1. The process-global state + the real core/apprt App pair, exactly
    //           as `gtk_first_light.rs` stands them up (see its section 0-1). ----
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

    // ---- 2. THE PRODUCT PATH UNDER TEST: `Window::new` + the first tab, the
    //         `performAction .new_window` shape (`application.zig:2285-2328`
    //         `initAndShowWindow`): config-bind, `newTabForWindow`, present.
    //         The command override pins a deterministic `/bin/sh` (the same
    //         override channel the reference threads through `newTabForWindow`,
    //         `window.zig:397-417`). ----
    let window = Window::new(&application, None);
    application
        .bind_property("config", &window, "config")
        .build();
    window.new_tab_for_window(
        None,
        TabOverrides {
            command: Some(Command::Direct(vec![b"/bin/sh".to_vec()])),
            working_directory: None,
            title: None,
        },
    );

    // (a) The window's TabView holds ONE page whose child is a Tab hosting a
    //     Surface (`window.zig:419-507` newTabPage + tab.zig:213-224 new).
    let tab_view = window.get_tab_view();
    assert_eq!(tab_view.n_pages(), 1, "new_tab_for_window adds one TabPage");
    let page = tab_view.selected_page().expect("the new page is selected");
    let tab = page
        .child()
        .downcast::<Tab>()
        .expect("the TabPage child is a Tab");
    let surface: Surface = tab
        .get_active_surface()
        .expect("the tab hosts an active Surface");

    window.present();

    // Keepalive + render pump (the first-light loop-driving discipline).
    let ctx = glib::MainContext::default();
    let keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);

    // ---- 3. Pump until the window chain realizes the GLArea → lazy core init.
    //         The split tree parents the Surface on an idle rebuild, so the
    //         GLArea exists only after some pumping. ----
    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
        ctx.iteration(true);
        if Instant::now() > deadline {
            break;
        }
    }

    // (b) The lazy core surface was built through the window chain.
    let core = surface.core().expect(
        "core surface built on the first GLArea resize INSIDE the window chain; \
         is LIBGL_ALWAYS_SOFTWARE=1 set so the GLArea reaches GL 4.3 core?",
    );

    // The Surface widget is parented INSIDE this window (the tab hosts it).
    {
        let mut ancestor = surface.clone().upcast::<gtk4::Widget>().parent();
        let mut in_window = false;
        while let Some(w) = ancestor {
            if w.clone().downcast::<Window>().is_ok() {
                in_window = true;
                break;
            }
            ancestor = w.parent();
        }
        assert!(
            in_window,
            "the Surface's widget ancestry reaches the Window"
        );
    }

    let gl_area = find_gl_area(surface.upcast_ref::<gtk4::Widget>())
        .expect("the surface template contains a GLArea");
    let render_pump = {
        let gl_area_weak = gl_area.downgrade();
        glib::timeout_add_local(Duration::from_millis(30), move || {
            match gl_area_weak.upgrade() {
                Some(area) => {
                    area.queue_render();
                    glib::ControlFlow::Continue
                }
                None => glib::ControlFlow::Break,
            }
        })
    };

    // ---- 4. (c) A real frame draws with GDK's GLArea framebuffer bound, and the
    //         readback of that framebuffer is not uniformly zero (the first-light
    //         capture shape: make_current + attach_buffers replicate GDK's
    //         `render` precondition). ----
    let mut draw_ok = false;
    let mut pixels_nonzero = 0usize;
    let capture_deadline = Instant::now() + Duration::from_secs(15);
    while !draw_ok && Instant::now() < capture_deadline {
        for _ in 0..8 {
            ctx.iteration(true);
        }
        if gl_area.error().is_some() {
            continue;
        }
        gl_area.make_current();
        if gl_area.error().is_some() {
            continue;
        }
        gl_area.attach_buffers();
        // SAFETY: `core` is the live core surface; the GLArea context is current
        // and its framebuffer bound (make_current + attach_buffers above). Raw
        // self per the W5 render-thread aliasing contract.
        unsafe {
            let draw =
                rustty_core::renderer_seam::Renderer::drawFrame(&raw mut (*core).renderer, true);
            let renderer = &(*core).renderer.0;
            let w = renderer.size.screen.width as usize;
            let h = renderer.size.screen.height as usize;
            if draw.is_ok() && w > 0 && h > 0 {
                let px = renderer.api.read_bound_framebuffer_rgba(w, h);
                pixels_nonzero = px.iter().filter(|&&b| b != 0).count();
                if pixels_nonzero > 0 {
                    draw_ok = true;
                }
            }
        }
    }
    assert!(
        draw_ok,
        "drawFrame(true) drew a real frame into the window's GLArea framebuffer"
    );
    assert!(
        pixels_nonzero > 0,
        "the GLArea framebuffer readback is not uniformly zero — the terminal \
         frame rendered INSIDE the window"
    );

    // ---- 5. (d) The live shell round-trip: `echo rustty-window-live` down the
    //         pty; the output round-trips through the io thread into the Screen. ----
    {
        let master = unsafe { pty_master(core) };
        // `ManuallyDrop` so the borrowed master fd is NOT closed here.
        let mut pipe = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(master) });
        pipe.write_all(b"echo rustty-window-live\n")
            .expect("write the echo command into the pty");
        pipe.flush().ok();
    }
    let round_trip_deadline = Instant::now() + Duration::from_secs(20);
    let mut round_tripped = false;
    while Instant::now() < round_trip_deadline {
        ctx.iteration(true);
        let dump = unsafe { screen_dump(core) };
        if dump.windows(NEEDLE.len()).any(|w| w == NEEDLE) {
            round_tripped = true;
            break;
        }
    }
    assert!(
        round_tripped,
        "live /bin/sh 'echo rustty-window-live' did not round-trip into the Screen \
         within the deadline; screen saw: {:?}",
        String::from_utf8_lossy(&unsafe { screen_dump(core) })
    );

    // Teardown: stop the helper timers, then `forget` the live objects — the
    // documented OUT-OF-SCOPE termio `Subprocess::drop` teardown gap
    // (`gtk_first_light.rs` section 6): a full drop of a live-shell surface hits
    // a pre-existing termio double-free entirely outside this tier. The OS
    // reclaims the threads + `/bin/sh` at process exit.
    keepalive.remove();
    render_pump.remove();
    drop(gl_area);
    std::mem::forget(surface);
    std::mem::forget(tab);
    std::mem::forget(page);
    std::mem::forget(tab_view);
    std::mem::forget(window);
    std::mem::forget(application);
    std::mem::forget(app);
    std::mem::forget(core_app);
}
