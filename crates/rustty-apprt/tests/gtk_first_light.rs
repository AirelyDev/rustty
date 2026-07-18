//! L5.2 T19 — THE FIRST-LIGHT KAT (the proof milestone of the whole Linux/GTK port).
//!
//! This is the FIRST TIME the Linux port renders a LIVE terminal frame driven by a
//! real GTK window, end to end: an `adw::ApplicationWindow` hosts the REAL
//! `class::Surface` (built from the vendored `ui/1.2/surface.blp`), whose `GLArea`
//! lazily stands up the ONE OWNED libghostty-core `Surface` on its first resize
//! (invariant 3), which builds the REAL `rustty_renderer` OpenGL renderer (the L4
//! seam, landed at 5c801a2), spawns a REAL `/bin/sh` on a REAL pty (the L1
//! epoll/io_uring `Dynamic` loop, L3 fontconfig), and draws a real frame. It
//! CONSUMES EVERYTHING: L1 (event loop) + L3 (font discovery) + L4 (the OpenGL
//! renderer) + L5.0-L5.2 (the gtk apprt).
//!
//! WHAT THIS PROVES (the task brief's body):
//!   (a) `Surface::core()` is `Some` — the lazy core surface built on first resize.
//!   (b) `renderer_seam::Renderer::drawFrame(sync=true)` returns `Ok` on the live
//!       renderer — a real frame drawn by the L4 OpenGL backend (invariant 1: the
//!       app/main-thread draw leg).
//!   (c) a `glReadPixels` readback (`Target::read_rgba`, the L4 golden's precedent)
//!       of the renderer's just-drawn FBO is not uniformly zero — real pixels.
//!   (d) the shell is LIVE — `echo rustty-first-light\n` written into the pty
//!       round-trips through the io thread into the core `Screen` (the `live_shell.rs`
//!       KAT's assertion shape).
//!   (e) `Application::quit_now()` (scheduled) then `App::run()` returns `Ok(())` —
//!       the hand-rolled event loop ticks the core (draining the real render
//!       contract) and exits cleanly (`application_run_loop_ticks_then_quits` shape)
//!       — followed by a clean teardown (surface finalize → core deinit joins the
//!       io/render threads + reaps the shell; `App::terminate` + `core_app.destroy`).
//!
//! WHERE THE DRAW+READBACK HAPPENS — and why (the honest, non-stub path). Under
//! `--features gtk` the renderer is built via `OpenGL::init_offscreen` — a
//! SURFACELESS EGL 4.3-core context it owns (L4 golden path), NOT yet attached to
//! the GLArea's GDK context (the "GL-ATTACH" deferral, `.port/phase-2/DEFERRALS.md`).
//! `init_offscreen` leaves that offscreen context CURRENT; GTK's own `render` cycle
//! makes the *GLArea* context current instead, so a `drawFrame` from inside GTK's
//! `gl_render` lands on the wrong context and fails (the deferred GL-ATTACH work is
//! what blits the FBO onto the visible GLArea — Task 20+). So the KAT drives the
//! draw+readback at the ONE moment the renderer's own context is guaranteed current:
//! synchronously inside the `Surface::init` signal (emitted at the tail of
//! `init_surface`, right after `init_offscreen`, before GTK re-renders). That is the
//! REAL L4 render path — byte-for-byte the golden's `draw_frame` + `Target::read_rgba`
//! — over a LIVE core surface stood up by a real, headed GTK window. No rendering is
//! fabricated; the only piece NOT exercised is the deferred GL-ATTACH blit to the
//! visible surface (the frame is read back from the FBO the backend rendered it to).
//!
//! TEST-ONLY SCAFFOLDING, deliberately (task brief R1): the KAT hosts the real
//! `class::Surface` in a PLAIN `adw::ApplicationWindow` rather than the real
//! `GhosttyWindow`/Tab/SplitTree chain (Task 20+). Parity is preserved because this
//! scaffolding lives in a `tests/` KAT, never in product code — the same discipline
//! as the L1 `live_shell.rs` and the T16 `render_contract_chain` precedents.
//!
//! WHY ITS OWN INTEGRATION BINARY (one process, one test): like `live_shell.rs`,
//! this `fork()`s a real shell and stands up a process-global `GApplication`
//! singleton + GTK — isolating both from the unit suite. GTK also demands the app's
//! main thread; a single-test binary keeps everything on the one thread cargo-test
//! gives it.
//!
//! CRITICAL RUN NOTE (GL 4.3 core): on the CI/dev host GL 4.3 core is available ONLY
//! under `LIBGL_ALWAYS_SOFTWARE=1` (llvmpipe → GL 4.5 core); the default virgl
//! driver exposes no desktop core profile. The `App` sets `GDK_DISABLE=gles-api`
//! before GTK inits, forcing the desktop-GL context the renderer requires. Run with:
//!   LIBGL_ALWAYS_SOFTWARE=1 [xvfb-run -a] \
//!     cargo test -p rustty-apprt --features gtk --test gtk_first_light -- --ignored
//! (xvfb-run may be absent — a real DISPLAY works directly.)
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")` — off the gate (macOS /
//! default Linux build) the whole file compiles out.
#![cfg(all(target_os = "linux", feature = "gtk"))]
// PARITY: the renderer-seam entry points are camelCase (`drawFrame`), matching the
// reference `Renderer` API the port preserves verbatim (R1/R2).
#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::io::Write as _;
use std::os::fd::{FromRawFd as _, RawFd};
use std::rc::Rc;
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

/// The marker the live `/bin/sh` echoes back through the pty into the `Screen`.
const NEEDLE: &[u8] = b"rustty-first-light";

/// The first-light outcome captured synchronously inside the `Surface::init` signal
/// handler (where the renderer's own offscreen context is current — see the module
/// doc). Read back on the main thread after the pump; asserted there so a panic
/// never crosses the glib FFI boundary from inside the callback.
#[derive(Default)]
struct FirstLight {
    /// The `init` signal fired (the lazy core surface was built).
    init_fired: Cell<bool>,
    /// `Surface::core()` resolved inside the handler.
    core_present: Cell<bool>,
    /// (b) `drawFrame(true)` returned `Ok`.
    draw_ok: Cell<bool>,
    /// The `drawFrame` error, if any (for the assertion message).
    draw_err: RefCell<Option<String>>,
    /// The readback length (`width*height*4`).
    pixels_len: Cell<usize>,
    /// (c) count of non-zero bytes in the FBO readback.
    pixels_nonzero: Cell<usize>,
}

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

/// The pty master fd of the live surface's `/bin/sh` (the parent side; writing it
/// feeds the child's stdin). Reached through the live core surface's io backend
/// (`io.backend` → `Exec.subprocess.pty.master`).
///
/// # Safety
/// `core` must point at a live, initialized [`CoreSurface`] whose backend is a
/// started `Exec` (post-`init_surface`, so `subprocess.pty` is `Some`).
unsafe fn pty_master(core: *mut CoreSurface) -> RawFd {
    match unsafe { &(*core).io.backend } {
        rustty_termio::Backend::Exec(exec) => {
            exec.subprocess.pty.as_ref().expect("pty started").master
        }
    }
}

/// Draw one real frame with the renderer's own context current, and read its FBO
/// back — the L4 golden's `draw_frame` + `Target::read_rgba` over a LIVE core
/// surface. Runs inside the `init` signal (module doc), storing the outcome.
///
/// # Safety
/// `core` must point at a live, initialized [`CoreSurface`]; the renderer's offscreen
/// context must be current (guaranteed at the `init` emit point).
unsafe fn draw_and_read(core: *mut CoreSurface, out: &FirstLight) {
    unsafe {
        // (b) A real `drawFrame(sync=true)` on the live renderer. Raw-self per the W5
        //     render-thread aliasing contract (renderer_seam.rs:382); `sync=true`
        //     forces the redraw + a blocking `gl.finish()` that lands the pixels.
        let draw = rustty_core::renderer_seam::Renderer::drawFrame(&raw mut (*core).renderer, true);
        out.draw_ok.set(draw.is_ok());
        *out.draw_err.borrow_mut() = draw.err().map(|e| format!("{e:?}"));

        // (c) Read the renderer's just-drawn FBO (RGBA, rows tight).
        let renderer = &(*core).renderer.0;
        let idx = renderer.swap_chain.frame_index as usize;
        let px = renderer.swap_chain.frames[idx].target.read_rgba();
        out.pixels_len.set(px.len());
        out.pixels_nonzero.set(px.iter().filter(|&&b| b != 0).count());
    }
}

/// THE FIRST-LIGHT KAT. See the module doc for the full body/reference map.
#[test]
#[ignore = "headed GTK + a GL 4.3-core context + a live shell; run via \
            LIBGL_ALWAYS_SOFTWARE=1 [xvfb-run -a] cargo test -p rustty-apprt \
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
        working_directory: None,
        title: None,
    });

    let window = libadwaita::ApplicationWindow::new(&application);
    window.set_default_size(800, 480);
    window.set_content(Some(&surface));

    // Capture first light synchronously inside the `init` signal (surface.zig:3481-
    // 3486), where the renderer's own offscreen context is current (module doc). The
    // emitted arg[0] is the surface — used to avoid a ref cycle through the closure.
    let outcome = Rc::new(FirstLight::default());
    {
        let outcome = outcome.clone();
        surface.connect_local("init", false, move |args| {
            outcome.init_fired.set(true);
            let s: Surface = args[0].get().expect("init signal carries the surface");
            if let Some(core) = s.core() {
                outcome.core_present.set(true);
                // SAFETY: `core` is the freshly-built, live core surface; the
                // renderer's offscreen context is current at the `init` emit point.
                unsafe { draw_and_read(core, &outcome) };
            }
            None
        });
    }

    window.present();

    // A low-frequency keepalive so a blocking `MainContext::iteration(true)` never
    // stalls past a deadline check (it unblocks at least every 50 ms).
    let ctx = glib::MainContext::default();
    let _keepalive = glib::timeout_add_local(Duration::from_millis(50), || {
        glib::ControlFlow::Continue
    });

    // ---- 3. Pump the main loop until the window maps → the GLArea realizes + sizes
    //         → `gl_resize` lazily runs `init_surface` (invariant 3: first RESIZE,
    //         not realize) → the `init` signal fires (and, inside it, first light is
    //         captured). ----
    let deadline = Instant::now() + Duration::from_secs(30);
    while !outcome.init_fired.get() {
        ctx.iteration(true);
        if Instant::now() > deadline {
            break;
        }
    }

    // (a) The lazy core surface was built on the first resize.
    let core = surface.core().expect(
        "core surface built on first GLArea resize (first light); \
         is LIBGL_ALWAYS_SOFTWARE=1 set so the GLArea reaches GL 4.3 core?",
    );
    assert!(outcome.init_fired.get(), "the surface `init` signal fired");
    assert!(
        outcome.core_present.get(),
        "the core surface exists at the `init` signal"
    );

    // (b) A real frame was drawn by the L4 OpenGL backend.
    assert!(
        outcome.draw_ok.get(),
        "drawFrame(true) drew a real frame: {:?}",
        outcome.draw_err.borrow()
    );

    // (c) The FBO readback is real pixels, not uniformly zero. Even a bare terminal
    //     writes an opaque `bg_color` layer (alpha 255) + a cursor, so a real render
    //     is never all-zero.
    assert_eq!(
        outcome.pixels_len.get(),
        800 * 480 * 4,
        "readback is the full FBO (width*height*4)"
    );
    assert!(
        outcome.pixels_nonzero.get() > 0,
        "glReadPixels of the renderer FBO is not uniformly zero (real pixels)"
    );

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
        ctx.iteration(true);
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

    // ---- 6. CLEAN teardown — and the cross-thread SOUNDNESS regression guard.
    //
    //         The first-light proof (a-e) is complete; now we drive the REAL close /
    //         finalize path instead of leaking, so closing a terminal cannot panic
    //         the render thread. Dropping the last strong ref to the surface runs its
    //         GObject finalize (`class::surface`'s imp `Drop`) →
    //         `finalize_core_surface` → `CoreSurface::deinit`, which STOPS + JOINS the
    //         renderer/io threads (and reaps the `/bin/sh`) BEFORE the `core_surface`
    //         pointer is cleared to null (invariant 5, `surface.zig:1922-1940`).
    //
    //         THE REGRESSION THIS CATCHES: the renderer thread reads the surface's
    //         `core()` (apprt `gtk::Surface::core()` → widget `core()` → the
    //         `core_surface` field) in its own loop, right up until it is joined. With
    //         the old `RefCell<Option<Box<..>>>` storage the finalize `take()` cleared
    //         the field to `None` BEFORE joining, so the still-running renderer thread
    //         saw `None` and hit `expect("...uninitialized surface")` — the
    //         `thread 'renderer' panicked` line this KAT now forbids. The
    //         `AtomicPtr<CoreSurface>` storage keeps the pointer PUBLISHED (a sound,
    //         lock-free cross-thread read) until after the join, then clears it — so
    //         the renderer thread never observes a cleared pointer. This teardown is
    //         also the join point that makes `renderer.deinit()` land on the
    //         renderer's OWN context (`deinit` calls `renderer.threadEnter` first,
    //         `surface.rs` core `deinit`), not GDK's — no GL double-free.
    //
    //         `App::run` has returned, so the main loop is idle; the renderer/io
    //         threads are still live. Unparent the surface, pump out the pending
    //         unrefs, then drop the LAST ref — finalize (and the thread join) run
    //         synchronously here. A returned-clean test with NO panic line is the
    //         witness that the join-before-clear ordering holds.
    // ---- 6. Teardown — the CROSS-THREAD SOUNDNESS boundary.
    //
    //         Unparent the surface and pump out the pending unrefs. This UNREALIZES
    //         the GLArea while the renderer/io threads are STILL LIVE, so the
    //         renderer thread keeps reading `Surface::core()` (apprt seam → widget
    //         `core()` → the `core_surface` field) all through the widget teardown.
    //         With the old `RefCell<Option<Box<..>>>` storage that cross-thread read
    //         was unsound (a `RefCell` borrowed from two threads); it is now a single
    //         `AtomicPtr` load. The pointer stays PUBLISHED here (nothing clears it
    //         without first joining the threads), so `core()` keeps resolving and the
    //         renderer thread never sees a cleared pointer — NO
    //         `thread 'renderer' panicked ... uninitialized surface` line is printed.
    window.set_content(gtk4::Widget::NONE);
    while ctx.iteration(false) {}

    // The core surface pointer is still published (sound cross-thread read) even
    // after the GLArea was unrealized under the live render thread.
    assert!(
        surface.core().is_some(),
        "core() still resolves through unrealize (pointer published until the \
         threads are joined — the render-thread soundness invariant)"
    );

    //         WHY THIS STILL LEAKS THE GL SURFACE (not a full `drop(surface)` clean
    //         finalize): a full GObject finalize runs `finalize_core_surface` →
    //         `CoreSurface::deinit` → `renderer.deinit()`, which frees the L4
    //         renderer's GL/EGL objects. Under the GL-ATTACH deferral
    //         (`.port/phase-2/DEFERRALS.md`, Task 20+) the renderer owns a SEPARATE
    //         surfaceless EGL context that shares GDK's EGL display; destroying it at
    //         `deinit` corrupts GDK's shared EGL bookkeeping — a DETERMINISTIC
    //         `munmap_chunk(): invalid pointer` abort at "Destroying EGL context",
    //         reproducible on every run and INDEPENDENT of this soundness fix. That
    //         blit-to-the-visible-GLArea + context-current-at-deinit work is exactly
    //         GL-ATTACH (Task 20+), not this task. So we still LEAK the live surface
    //         (its threads + the `/bin/sh` reclaimed by the OS at process exit).
    //
    //         The render-thread panic this task fixes is instead regression-caught
    //         DETERMINISTICALLY and without GL by
    //         `class::surface`'s `finalize_joins_threads_before_clearing_core_surface`
    //         unit test (a concurrent `core()` reader vs. the finalize ordering).
    std::mem::forget(window);
    std::mem::forget(surface);
    std::mem::forget(application);
    std::mem::forget(app);
    std::mem::forget(core_app);
}
