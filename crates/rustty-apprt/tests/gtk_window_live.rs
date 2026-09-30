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
//!       RenderSurface's first resize (invariant 3), reached THROUGH the window
//!       chain.
//!   (c) a real frame shows: the `RenderSurface` built a `GdkTexture` from a frame
//!       the render thread exported, and its pixels are not uniformly zero (the
//!       render contract runs; P7-W7 T2, `2f0b65346`).
//!   (d) the shell is LIVE — `echo rustty-window-live\n` written into the pty
//!       round-trips through the io thread into the core `Screen`.
//!   (e) the surface CLOSES the way the app closes it: the window is destroyed,
//!       the `Surface` GObject is finalized (no leaked reference), and its shell
//!       is killed and reaped.
//!
//! The texture inspection and the run notes are `gtk_first_light.rs`'s — see its
//! module doc for the full rationale. Run with:
//!   LIBGL_ALWAYS_SOFTWARE=1 \
//!     cargo test -p rustty-apprt --features gtk --test gtk_window_live -- --ignored
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")` — off the gate the whole
//! file compiles out.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
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
use rustty_apprt::gtk::class::overrides::TabOverrides;
use rustty_apprt::gtk::class::surface::{CoreSurface, Surface};
use rustty_apprt::gtk::class::tab::Tab;
use rustty_apprt::gtk::class::window::Window;
use rustty_apprt::gtk::{self, register_resources};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;

#[path = "common/gtk_live.rs"]
mod gtk_live;

/// The marker the live `/bin/sh` echoes back through the pty into the `Screen`.
const NEEDLE: &[u8] = b"rustty-window-live";

/// Dump the live core surface's ACTIVE `Screen` text UNDER THE RENDERER MUTEX
/// (the `live_shell.rs`/first-light determinism discipline).
///
/// # Safety
/// `core` must point at a live, initialized [`CoreSurface`]; called on the main
/// thread while the io thread is the only other toucher (under the mutex).
unsafe fn screen_dump(core: *mut CoreSurface) -> Vec<u8> {
    unsafe {
        let mutex = (*(*core).renderer_state.as_ptr()).mutex;
        (*mutex).lock();
        let active = (*(*(*core).renderer_state.as_ptr()).terminal)
            .screens
            .active;
        let dump = (*active).dump_string_alloc(
            Global.into(),
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
    match unsafe { &(*(*core).io.as_ptr()).backend } {
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
            shell_integration: None,
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

    // Stops the render thread if an assertion unwinds past the close below
    // (P7-W7 T6, N450). Declared after `core_app` and `app`, so it drops first.
    let render_threads = gtk_live::RenderThreadGuard::new(core_ptr, [&surface]);

    // Keepalive + render pump (the first-light loop-driving discipline).
    let ctx = glib::MainContext::default();
    let keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);

    // ---- 3. Pump until the window chain sizes the RenderSurface → lazy core
    //         init. The split tree parents the Surface on an idle rebuild, so the
    //         RenderSurface is sized only after some pumping. ----
    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
        gtk_live::iterate(&ctx, core_ptr, &app);
        if Instant::now() > deadline {
            break;
        }
    }

    // (b) The lazy core surface was built through the window chain.
    let core = surface.core().expect(
        "core surface built on the first RenderSurface resize INSIDE the window \
         chain; is LIBGL_ALWAYS_SOFTWARE=1 set so the renderer reaches GL 4.3 core?",
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

    let render_surface = gtk_live::find_render_surface(surface.upcast_ref::<gtk4::Widget>())
        .expect("the surface template contains a RenderSurface");

    // ---- 4. (c) A real frame shows: the RenderSurface's texture, built in its
    //         snapshot from a frame the render thread exported, is not uniformly
    //         zero. ----
    let mut pixels_nonzero = 0usize;
    let capture_deadline = Instant::now() + Duration::from_secs(15);
    while pixels_nonzero == 0 && Instant::now() < capture_deadline {
        gtk_live::iterate(&ctx, core_ptr, &app);
        if let Some(texture) = render_surface.texture() {
            let (_, _, px) = gtk_live::texture_rgba(&texture);
            pixels_nonzero = gtk_live::nonzero_bytes(&px);
        }
    }
    assert!(
        pixels_nonzero > 0,
        "the RenderSurface texture is not uniformly zero — the terminal frame \
         rendered INSIDE the window"
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
        gtk_live::iterate(&ctx, core_ptr, &app);
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

    // ---- 6. (e) Teardown: close the surface the way the app does. Stop the
    //         helper timer, drop the KAT's other widget references, destroy the
    //         window, and let the last reference finalize the `Surface`:
    //         `deleteSurface`, then `CoreSurface::deinit` (stop and join the
    //         threads, kill and reap the shell), then the core surface is freed.
    //         `close_surface` asserts both. The apps then tear down in
    //         `run_gtk`'s order. (This KAT used to `forget` everything here, to
    //         dodge a termio `Subprocess` double-free that has since been fixed;
    //         P7-W7 T6.) ----
    // SAFETY: `core` is the live core surface; its shell was started by init.
    let shell_pid = unsafe { gtk_live::shell_pid(core) };
    keepalive.remove();
    drop(render_surface);
    drop(tab);
    drop(page);
    drop(tab_view);
    gtk_live::close_surface(&ctx, window, surface, shell_pid);
    gtk_live::close_other_windows(&ctx);
    drop(render_threads);
    app.terminate();
    core_app.destroy();
}
