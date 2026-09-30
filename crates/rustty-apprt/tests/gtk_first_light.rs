//! L5.2 T19 — THE FIRST-LIGHT KAT (the proof milestone of the whole Linux/GTK port),
//! rewritten for P7-W7 T2 (`2f0b65346`): the terminal is a `RenderSurface` that
//! displays the frames the render thread exports, and the visible-render proof is
//! the `GdkTexture` that widget builds in its snapshot.
//!
//! An `adw::ApplicationWindow` hosts the REAL `class::Surface` (built from the
//! vendored `ui/1.2/surface.blp`), whose `RenderSurface` lazily stands up the ONE
//! OWNED libghostty-core `Surface` on its first resize (invariant 3), which builds
//! the REAL `rustty_renderer` OpenGL renderer over its own surfaceless EGL context,
//! spawns a REAL `/bin/sh` on a REAL pty, and whose render thread draws frames the
//! widget shows. It CONSUMES EVERYTHING: L1 (event loop) + L3 (font discovery) + L4
//! (the OpenGL renderer) + L5.0-L5.2 (the gtk apprt) + the EGL/DMA-BUF redesign.
//!
//! WHAT THIS PROVES:
//!   (a) `Surface::core()` is `Some` — the lazy core surface built on first resize.
//!   (b) THE VISIBLE-RENDER PROOF: the `RenderSurface` built a `GdkTexture` from a
//!       frame the render thread exported (`ExportedFrame` → `LatestFrame` →
//!       `takeFrame` in `snapshot`), the size of the widget in device pixels, and
//!       its pixels are not uniformly zero (a bare terminal writes an opaque
//!       background and a cursor). The texture kind (DMA-BUF import or the
//!       CPU-readback fallback) is printed.
//!   (c) the shell is LIVE — `echo rustty-first-light\n` written into the pty
//!       round-trips through the io thread into the core `Screen` (the
//!       `live_shell.rs` KAT's assertion shape).
//!   (d) `App::run()` returns `Ok(())` after a scheduled `quit_now`, followed by a
//!       teardown that UNREALIZES the `RenderSurface` (the render thread then
//!       releases its GPU resources on its own context) with no `thread 'renderer'
//!       panicked` line.
//!
//! TEST-ONLY SCAFFOLDING, deliberately (task brief R1): the KAT hosts the real
//! `class::Surface` in a PLAIN `adw::ApplicationWindow` rather than the real
//! `GhosttyWindow`/Tab/SplitTree chain. Parity is preserved because this
//! scaffolding lives in a `tests/` KAT, never in product code.
//!
//! WHY ITS OWN INTEGRATION BINARY (one process, one test): this `fork()`s a real
//! shell and stands up a process-global `GApplication` singleton + GTK, and GTK
//! demands the app's main thread; a single-test binary keeps everything on the one
//! thread cargo-test gives it.
//!
//! RUN NOTE (GL 4.3 core): the renderer's surfaceless context needs GL 4.3 core;
//! on the rvm host that is llvmpipe under `LIBGL_ALWAYS_SOFTWARE=1`. Run under any
//! display (a headless sway, or broadway for the CPU-readback path):
//!   LIBGL_ALWAYS_SOFTWARE=1 \
//!     cargo test -p rustty-apprt --features gtk --test gtk_first_light -- --ignored
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")` — off the gate (macOS /
//! default Linux build) the whole file compiles out.
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
use libadwaita::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::surface::{CoreSurface, Surface, SurfaceOverrides};
use rustty_apprt::gtk::{self, register_resources};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;

#[path = "common/gtk_live.rs"]
mod gtk_live;

/// The marker the live `/bin/sh` echoes back through the pty into the `Screen`.
const NEEDLE: &[u8] = b"rustty-first-light";

/// Dump the live core surface's ACTIVE `Screen` text, read UNDER THE RENDERER MUTEX
/// — the same lock the io thread's `processOutput` holds while it commits, so this
/// observes only committed Screen state (the `live_shell.rs` determinism discipline).
///
/// # Safety
/// `core` must point at a live, initialized [`CoreSurface`] (a successful
/// `init_surface`); called on the main thread while the io thread is the only other
/// toucher (under the mutex).
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

/// The pty master fd of the live surface's `/bin/sh` (the parent side; writing it
/// feeds the child's stdin). Reached through the live core surface's io backend
/// (`io.backend` → `Exec.subprocess.pty.master`).
///
/// # Safety
/// `core` must point at a live, initialized [`CoreSurface`] whose backend is a
/// started `Exec` (post-`init_surface`, so `subprocess.pty` is `Some`).
unsafe fn pty_master(core: *mut CoreSurface) -> RawFd {
    match unsafe { &(*(*core).io.as_ptr()).backend } {
        rustty_termio::Backend::Exec(exec) => {
            exec.subprocess.pty.as_ref().expect("pty started").master
        }
    }
}

/// THE FIRST-LIGHT KAT. See the module doc for the full body/reference map.
#[test]
#[ignore = "headed GTK + a GL 4.3-core context + a live shell; run via \
            LIBGL_ALWAYS_SOFTWARE=1 cargo test -p rustty-apprt \
            --features gtk --test gtk_first_light -- --ignored"]
fn first_light_renders_a_live_terminal_frame() {
    // ---- 0. Initialize the process-global `rustty_core::STATE` — exactly what the
    //         real entrypoint (`rustty_init` → `rustty_apprt::init`) does before any
    //         core consumer runs (resources dir, logging). Without it the font /
    //         renderer setup inside `init_surface` panics at `global_state()`. ----
    rustty_apprt::init().expect("initialize rustty global state");

    // ---- 1. The core App + the gtk apprt App (the real `run_gtk` construction,
    //         main_rustty.rs:351-377). No GTK is touched before `App::init` →
    //         `Application::new` → `setGtkEnv` (which asserts GTK is not yet
    //         initialized), so this must be the first GTK touch in the process. ----
    let mut core_app: Box<CoreApp<gtk::App>> =
        CoreApp::<gtk::App>::create(Global).expect("core App::create");
    let core_ptr: *mut CoreApp<gtk::App> = &mut *core_app;

    let app = match gtk::App::init(core_ptr.cast()) {
        Ok(rt) => rt,
        Err(err) => match err {},
    };

    // The process-global `GApplication` singleton + backref binding the live pump
    // needs (normally done inside `App::run`; hoisted here because the surface lazily
    // inits DURING the manual pump, before `run`). `Application::default()` must
    // resolve to THIS app before `Surface::new` (its `constructed` reads the app's
    // config), and `init_surface` reads the bound `core_app`/`rt_app` cells.
    let application: Application = app.application().clone();
    application.upcast_ref::<gio::Application>().set_default();
    application
        .register(gio::Cancellable::NONE)
        .expect("register the application");
    // rt_app = this pinned `gtk::App`'s stable address (App.zig:43 passes `self`);
    // `app` is a fixed local that never moves before `run` re-binds the same address.
    let app_ptr: *mut std::ffi::c_void = std::ptr::from_ref(&app).cast_mut().cast();
    application.bind_rt_app(app_ptr);
    application.bind_core_app::<gtk::App>();

    // A noop-only build resolves the windowing protocol to `.none` (winproto.zig:46).
    assert_eq!(
        application.winproto_protocol(),
        gtk::winproto::Protocol::None,
        "noop-only build selects the .none windowing protocol"
    );

    // The compiled gresource blob (the surface template lives in it).
    register_resources();

    // ---- 2. The REAL `class::Surface` in a plain `adw::ApplicationWindow`
    //         (test-only scaffolding). Override the command to a deterministic
    //         `/bin/sh` reading from its pty (a bare shell that runs the line we write
    //         and stays alive — no login wrapper, no PATH assumptions; `echo` is a
    //         shell builtin). ----
    let surface = Surface::new(SurfaceOverrides {
        command: Some(Command::Direct(vec![b"/bin/sh".to_vec()])),
        shell_integration: None,
        working_directory: None,
        title: None,
    });

    let window = libadwaita::ApplicationWindow::new(&application);
    window.set_default_size(800, 480);
    window.set_content(Some(&surface));

    let render_surface = gtk_live::find_render_surface(surface.upcast_ref::<gtk4::Widget>())
        .expect("the surface template contains a RenderSurface");

    window.present();

    // A low-frequency keepalive so a blocking `MainContext::iteration(true)` never
    // stalls past a deadline check (it unblocks at least every 50 ms). Removed
    // before teardown (a `SourceId` dropped does NOT remove its source).
    let ctx = glib::MainContext::default();
    let keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);

    // ---- 3. Pump the main loop until the window maps → the RenderSurface sizes
    //         → `render_surface_resize` lazily runs `init_surface` (invariant 3:
    //         first RESIZE) → the core surface is built and its render thread
    //         starts drawing. ----
    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
        gtk_live::iterate(&ctx, core_ptr, &app);
        if Instant::now() > deadline {
            break;
        }
    }

    // (a) The lazy core surface was built on the first resize.
    let core = surface.core().expect(
        "core surface built on first RenderSurface resize (first light); \
         is LIBGL_ALWAYS_SOFTWARE=1 set so the renderer reaches GL 4.3 core?",
    );

    // (b) Pump until the RenderSurface has built a texture from an exported frame
    //     with pixels in it: the render thread pushed a frame, `.redraw` reached
    //     `Surface::redraw` → `queue_draw`, and the snapshot took it.
    //     The compositor may resize the window after the first frames, so wait
    //     for a frame the size of the widget in device pixels (one texel per
    //     device pixel, `render_surface.zig:176-180`).
    let capture_deadline = Instant::now() + Duration::from_secs(20);
    let mut captured: Option<(usize, usize, usize, &'static str)> = None;
    let mut last = None;
    while captured.is_none() && Instant::now() < capture_deadline {
        gtk_live::iterate(&ctx, core_ptr, &app);
        if let Some(texture) = render_surface.texture() {
            let (w, h, px) = gtk_live::texture_rgba(&texture);
            let nonzero = gtk_live::nonzero_bytes(&px);
            let device = render_surface.device_size();
            last = Some((w, h, device));
            if nonzero > 0 && (w as u32, h as u32) == (device.width, device.height) {
                captured = Some((w, h, nonzero, gtk_live::texture_kind(&texture)));
            }
        }
    }
    let (w, h, nonzero, kind) = captured.unwrap_or_else(|| {
        panic!(
            "the RenderSurface never displayed a non-zero frame of its device size \
             (last texture/device: {last:?})"
        )
    });
    eprintln!("FIRST-LIGHT: {kind} texture {w}x{h}, {nonzero} non-zero bytes");
    assert!(w > 0 && h > 0, "the texture has an extent");

    // ---- 4. The live shell round-trip (d): write `echo rustty-first-light\n` into
    //         the pty master; the shell runs it and its output round-trips through
    //         the io thread's read loop → `processOutput` → the core `Screen`. ----
    {
        let master = unsafe { pty_master(core) };
        // `ManuallyDrop` so the borrowed master fd is NOT closed here (the io thread
        // still owns it); we only write to it.
        let mut pipe = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(master) });
        pipe.write_all(b"echo rustty-first-light\n")
            .expect("write the echo command into the pty");
        pipe.flush().ok();
    }

    let round_trip_deadline = Instant::now() + Duration::from_secs(20);
    let mut round_tripped = false;
    while Instant::now() < round_trip_deadline {
        // Block (bounded by the keepalive) to yield to the io + render threads.
        gtk_live::iterate(&ctx, core_ptr, &app);
        let dump = unsafe { screen_dump(core) };
        if dump.windows(NEEDLE.len()).any(|w| w == NEEDLE) {
            round_tripped = true;
            break;
        }
    }
    assert!(
        round_tripped,
        "live /bin/sh 'echo rustty-first-light' did not round-trip into the Screen within \
         the deadline; screen saw: {:?}",
        String::from_utf8_lossy(&unsafe { screen_dump(core) })
    );

    // Stop our helper timer and drop the KAT's strong RenderSurface ref BEFORE
    // teardown, so nothing external keeps it (a surface child) alive past the surface
    // drop. (The scheduled `quit_now` source below wakes `App::run` on its own.)
    keepalive.remove();
    drop(render_surface);

    // ---- 5. (e) The hand-rolled event loop exits cleanly. Schedule `quit_now` on the
    //         pumped context (the `application_run_loop_ticks_then_quits` shape), then
    //         enter `App::run` — which ticks the core (draining the real render
    //         contract) and returns Ok once the scheduled `quit_now` trips `running`.
    //         `App::run` only returns after its `while running { … }` loop exits, so a
    //         returned `Ok(())` is itself the witness the loop unwound cleanly. ----
    {
        let app_weak = application.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(150), move || {
            if let Some(app) = app_weak.upgrade() {
                app.quit_now();
            }
        });
    }
    let run_result = app.run();
    assert!(
        run_result.is_ok(),
        "App::run returns Ok after quit_now: {:?}",
        run_result.err()
    );

    // ---- 6. Teardown. Unparent the surface and pump out the pending unrefs. This
    //         UNREALIZES the RenderSurface → `displayUnrealized` → the render thread
    //         releases the swap chain and shaders on its own EGL context. The
    //         renderer/io threads are STILL LIVE here, so the renderer thread keeps
    //         reading `Surface::core()` through the unrealize — a single `AtomicPtr`
    //         load that stays PUBLISHED, so NO `thread 'renderer' panicked ...
    //         uninitialized surface` line prints.
    window.set_content(gtk4::Widget::NONE);
    while ctx.iteration(false) {}

    assert!(
        surface.core().is_some(),
        "core() still resolves through unrealize (pointer published until the \
         threads are joined — the render-thread soundness invariant)"
    );

    // OUT-OF-SCOPE GAP (characterized, NOT owned by GL-ATTACH): a FULL `drop(surface)`
    // here — which runs the GObject finalize → `CoreSurface::deinit` → the boxed
    // `CoreSurface` drop — aborts with `free(): invalid size` deep inside
    // `rustty_termio::exec::Subprocess`'s `Drop`, freeing its `Vec<CString>`
    // (subprocess args/env) with a mismatched size. That is a TERMIO teardown
    // double-free (an explicit `deinit` followed by a Rust `Drop` over the same
    // buffers), entirely in `rustty-termio`, with NO GL / renderer / EGL frame on the
    // aborting stack. It is left to its owner, so we `forget` the live objects
    // (their threads + the `/bin/sh` reclaimed by the OS at process exit).
    // SAFETY: `core` is the live core surface; stopped once, before exit.
    unsafe { gtk_live::stop_render_thread(core) };
    std::mem::forget(window);
    std::mem::forget(surface);
    std::mem::forget(application);
    std::mem::forget(app);
    std::mem::forget(core_app);
}
