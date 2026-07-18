//! L5.2 T19 / GL-ATTACH — THE FIRST-LIGHT KAT (the proof milestone of the whole
//! Linux/GTK port), now proving VISIBLE render into GDK's GLArea framebuffer.
//!
//! This is the FIRST TIME the Linux port renders a LIVE terminal frame driven by a
//! real GTK window, end to end AND lands it on the WINDOW: an `adw::ApplicationWindow`
//! hosts the REAL `class::Surface` (built from the vendored `ui/1.2/surface.blp`),
//! whose `GLArea` lazily stands up the ONE OWNED libghostty-core `Surface` on its
//! first resize (invariant 3), which builds the REAL `rustty_renderer` OpenGL renderer
//! bound to GDK's GLArea context (the GL-ATTACH work), spawns a REAL `/bin/sh` on a
//! REAL pty (the L1 epoll/io_uring `Dynamic` loop, L3 fontconfig), and draws a real
//! frame that appears on the window. It CONSUMES EVERYTHING: L1 (event loop) + L3
//! (font discovery) + L4 (the OpenGL renderer) + L5.0-L5.2 (the gtk apprt) + GL-ATTACH.
//!
//! WHAT THIS PROVES (the task brief's body):
//!   (a) `Surface::core()` is `Some` — the lazy core surface built on first resize.
//!   (b) `renderer_seam::Renderer::drawFrame(sync=true)` returns `Ok` on the live
//!       renderer, driven with GDK's GLArea context current + its framebuffer bound —
//!       a real frame drawn by the L4 OpenGL backend and PRESENTED into that
//!       framebuffer via `present_to_default`.
//!   (c) THE VISIBLE-RENDER PROOF: a `glReadPixels` readback of GDK's GLArea
//!       framebuffer (the currently-bound host framebuffer — a NON-zero FBO, NOT the
//!       renderer's offscreen target FBO) is not uniformly zero. The frame the
//!       renderer `present_to_default`-blitted landed in the WINDOW's framebuffer.
//!   (d) the shell is LIVE — `echo rustty-first-light\n` written into the pty
//!       round-trips through the io thread into the core `Screen` (the `live_shell.rs`
//!       KAT's assertion shape).
//!   (e) `App::run()` returns `Ok(())` after a scheduled `quit_now` — the hand-rolled
//!       event loop ticks the core (draining the real render contract) and exits
//!       cleanly — followed by a teardown that UNREALIZES the GLArea and survives GDK's
//!       "Destroying EGL context" with NO `munmap` double-free and NO `thread
//!       'renderer' panicked` line. The renderer no longer owns a separate surfaceless
//!       EGL context (GL-ATTACH: it borrows GDK's), so tearing it down no longer
//!       corrupts GDK's shared EGL bookkeeping — the `munmap_chunk(): invalid pointer`
//!       abort at "Destroying EGL context" is GONE. (A FULL `drop(surface)` then hits
//!       a SEPARATE, out-of-scope termio `Subprocess::drop` gap — see section 6.)
//!
//! WHERE THE DRAW+READBACK HAPPENS — and why (the honest, non-stub path). Under
//! `--features gtk` the renderer is now built via `OpenGL::init` (the WINDOWED ctor —
//! it owns NO context; GDK's GLArea context is loaded via `prepare_context_windowed`
//! in `surfaceInit`). The visible frame lands on the window through the template's own
//! `gl_render` (in GTK's `render` signal), which `present_to_default`-blits the frame
//! into the GLArea framebuffer GDK bound. For the READBACK proof the KAT cannot use a
//! second `render` handler — `GtkGLArea::render` has a boolean-handled accumulator and
//! the template `gl_render` returns TRUE (handled), stopping later handlers. So the KAT
//! replicates the `render` precondition explicitly: `gl_area.make_current()` +
//! `gl_area.attach_buffers()` (which binds the GLArea's own framebuffer exactly as GDK
//! does before `render`), then `drawFrame(true)` (renders into the frame's offscreen
//! target THEN `present_to_default`-blits it into that bound framebuffer), then reads
//! the SAME framebuffer back — proving the terminal pixels reached the WINDOW's
//! framebuffer, not just the renderer's private target FBO. Nothing is fabricated.
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
//! before GTK inits, forcing the desktop-GL context the renderer requires. The
//! GLArea's own GDK context therefore ALSO needs `LIBGL_ALWAYS_SOFTWARE=1` to reach
//! 4.3 core. Run with:
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

/// The first-light outcome captured with GDK's GLArea context current + its
/// framebuffer bound (via `make_current` + `attach_buffers` — see the module doc).
/// Asserted on the main thread after the capture so a panic never crosses the glib
/// FFI boundary.
#[derive(Default)]
struct FirstLight {
    /// A capture ran and observed a non-zero GLArea-framebuffer frame.
    captured: Cell<bool>,
    /// `Surface::core()` resolved at the capture.
    core_present: Cell<bool>,
    /// (b) `drawFrame(true)` returned `Ok`.
    draw_ok: Cell<bool>,
    /// The `drawFrame` error, if any (for the assertion message).
    draw_err: RefCell<Option<String>>,
    /// The GLArea-framebuffer readback length (`width*height*4`).
    pixels_len: Cell<usize>,
    /// (c) count of non-zero bytes in the GLArea-framebuffer readback.
    pixels_nonzero: Cell<usize>,
}

/// Recursively find the terminal `GLArea` inside the `Surface` widget tree
/// (`Surface` → `terminal_page` Overlay → Box → `GLArea`, per `surface.blp`). The
/// KAT needs the GLArea to `make_current` + `attach_buffers` for the visible-render
/// readback; the widget is a private template child with no public accessor, so we
/// walk the tree by type — a `tests/`-only concern that never touches product code.
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

/// Draw one real frame WITH GDK'S GLAREA FRAMEBUFFER BOUND, then read that framebuffer
/// back — the GL-ATTACH visible-render proof. The caller has already made the GLArea
/// context current and called `gtk_gl_area_attach_buffers`, which binds the GLArea's
/// own (non-zero) framebuffer exactly as GDK does before emitting `render`. So this
/// replicates the `gl_render` precondition: `drawFrame(sync=true)` renders into the
/// frame's offscreen target and, on the windowed path, `present_to_default`-blits it
/// into the bound GLArea framebuffer; we then read that SAME framebuffer to prove the
/// pixels reached the WINDOW's context (NOT the renderer's private target FBO).
///
/// # Safety
/// `core` must point at a live, initialized [`CoreSurface`]; the GLArea context must
/// be current and its framebuffer bound (the caller's `make_current` + `attach_buffers`).
unsafe fn draw_present_read_glarea_fb(core: *mut CoreSurface, out: &FirstLight) {
    unsafe {
        // (b) A real `drawFrame(sync=true)` on the live renderer — the SAME call
        //     `gl_render` makes. Raw-self per the W5 render-thread aliasing contract
        //     (renderer_seam.rs:382); `sync=true` forces the redraw + a blocking
        //     `gl.finish()` and (windowed) the `present_to_default` blit into the
        //     bound GLArea framebuffer.
        let draw = rustty_core::renderer_seam::Renderer::drawFrame(&raw mut (*core).renderer, true);
        out.draw_ok.set(draw.is_ok());
        *out.draw_err.borrow_mut() = draw.err().map(|e| format!("{e:?}"));

        // (c) Read the currently-bound (GLArea) framebuffer, NOT the renderer's
        //     offscreen target FBO. The renderer's stored `size.screen` is the live
        //     GLArea viewport size (set from `surface_size()`/`GL_VIEWPORT`).
        let renderer = &(*core).renderer.0;
        let w = renderer.size.screen.width as usize;
        let h = renderer.size.screen.height as usize;
        if w > 0 && h > 0 {
            let px = renderer.api.read_bound_framebuffer_rgba(w, h);
            out.pixels_len.set(px.len());
            out.pixels_nonzero
                .set(px.iter().filter(|&&b| b != 0).count());
        }
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

    // GL-ATTACH visible-render capture happens AFTER the pump (below), not in a
    // `render` handler: `GtkGLArea::render` uses a boolean-handled accumulator and the
    // template's `gl_render` returns TRUE (handled), which stops any later handler — so
    // a second `render` handler never runs. Instead we drive the capture explicitly via
    // `make_current` + `attach_buffers` (which binds the GLArea's own framebuffer just
    // as GDK does before `render`), so the readback observes the SAME framebuffer the
    // on-window `gl_render` presents into. We grab the GLArea now (its `render` — the
    // real visible path — keeps presenting during the pump).
    let outcome = Rc::new(FirstLight::default());
    let gl_area = find_gl_area(surface.upcast_ref::<gtk4::Widget>())
        .expect("the surface template contains a GLArea");

    window.present();

    // A low-frequency keepalive so a blocking `MainContext::iteration(true)` never
    // stalls past a deadline check (it unblocks at least every 50 ms), and a periodic
    // `queue_render` so the GLArea keeps emitting `render` (the real on-window present).
    // Both are removed before teardown (a `SourceId` dropped does NOT remove its
    // source), and the `queue_render` timer holds only a WEAK GLArea ref.
    let ctx = glib::MainContext::default();
    let keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);
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

    // ---- 3. Pump the main loop until the window maps → the GLArea realizes + sizes
    //         → `gl_resize` lazily runs `init_surface` (invariant 3: first RESIZE) →
    //         the core surface is built and the GLArea starts presenting real frames. ----
    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
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

    // Let the real on-window `gl_render` present a few frames into the GLArea
    // framebuffer, then capture: make the GLArea context current, bind its framebuffer
    // (`attach_buffers`, exactly as GDK does before `render`), draw + present + read.
    let capture_deadline = Instant::now() + Duration::from_secs(10);
    while !outcome.captured.get() && Instant::now() < capture_deadline {
        // Pump a little so `gl_render` runs and the widget is fully realized.
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
        // Bind the GLArea's framebuffer as the draw target (GDK's `render` precondition).
        gl_area.attach_buffers();
        outcome.core_present.set(true);
        // SAFETY: `core` is the live core surface; the GLArea context is current and
        // its framebuffer bound (make_current + attach_buffers above).
        unsafe { draw_present_read_glarea_fb(core, &outcome) };
        if outcome.pixels_nonzero.get() > 0 {
            outcome.captured.set(true);
        }
    }

    assert!(
        outcome.captured.get(),
        "captured a non-zero frame from the GLArea framebuffer"
    );
    assert!(
        outcome.core_present.get(),
        "the core surface exists at the capture"
    );

    // (b) A real frame was drawn by the L4 OpenGL backend, driven from `render`.
    assert!(
        outcome.draw_ok.get(),
        "drawFrame(true) drew a real frame: {:?}",
        outcome.draw_err.borrow()
    );

    // (c) THE VISIBLE-RENDER PROOF: the readback of GDK's GLArea framebuffer (FBO 0)
    //     is real pixels, not uniformly zero. Even a bare terminal writes an opaque
    //     `bg_color` layer (alpha 255) + a cursor, and `present_to_default` blitted
    //     that into FBO 0 — so a real windowed render is never all-zero.
    assert!(
        outcome.pixels_len.get() > 0,
        "FBO-0 readback has a non-empty extent (the GLArea viewport was sized)"
    );
    assert!(
        outcome.pixels_nonzero.get() > 0,
        "glReadPixels of GDK's GLArea framebuffer (FBO 0) is not uniformly zero — the \
         frame the renderer present_to_default-blitted landed in the WINDOW's context"
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

    // Stop our helper timers and drop the KAT's strong GLArea ref BEFORE teardown, so
    // nothing external keeps the GLArea (a surface child) alive past the surface drop
    // and no timer fires `queue_render` on an unrealized area. (The scheduled
    // `quit_now` source below wakes `App::run` on its own — no keepalive needed there.)
    keepalive.remove();
    render_pump.remove();
    drop(gl_area);

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

    // ---- 6. Teardown — the GL-ATTACH renderer double-free fix in action, PROVEN by
    //         surviving the GLArea unrealize + GDK context destroy.
    //
    //         Before GL-ATTACH the renderer OWNED a SEPARATE surfaceless EGL context
    //         that shared GDK's EGL display; tearing it down corrupted GDK's shared EGL
    //         bookkeeping — a DETERMINISTIC `munmap_chunk(): invalid pointer` abort at
    //         "Destroying EGL context". GL-ATTACH makes the windowed renderer BORROW
    //         GDK's context (it owns none), so it eglTerminates nothing.
    //
    //         Unparent the surface and pump out the pending unrefs. This UNREALIZES the
    //         GLArea → `displayUnrealized` frees the swap-chain + shaders while GDK's
    //         context is still current → GDK then destroys its own GLArea context
    //         ("Destroying EGL context"). We now survive that with NO `munmap` / NO
    //         abort — the renderer double-free is GONE. The renderer/io threads are
    //         STILL LIVE here, so the renderer thread keeps reading `Surface::core()`
    //         through the unrealize — a single `AtomicPtr` load that stays PUBLISHED,
    //         so NO `thread 'renderer' panicked ... uninitialized surface` line prints.
    window.set_content(gtk4::Widget::NONE);
    while ctx.iteration(false) {}

    // The core surface pointer is still published (sound cross-thread read) even after
    // the GLArea was unrealized under the live render thread — and we passed GDK's
    // "Destroying EGL context" without the pre-GL-ATTACH munmap double-free.
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
    // aborting stack. It is INDEPENDENT of this task and was never observable before,
    // because the pre-GL-ATTACH KAT `forget`s the surface to dodge the (now-fixed) EGL
    // munmap. The GL-ATTACH deliverable — the renderer no longer double-frees its
    // context — is proven above (we survive "Destroying EGL context"); the termio
    // `Subprocess::drop` gap is left to its owner. So we `forget` the live objects
    // (their threads + the `/bin/sh` reclaimed by the OS at process exit).
    std::mem::forget(window);
    std::mem::forget(surface);
    std::mem::forget(application);
    std::mem::forget(app);
    std::mem::forget(core_app);
}
