//! Live KATs for the `RenderSurface` presentation paths (P7-W7 T2, R2 §3-4,
//! Review Focus 1 and 2).
//!
//! * `render_surface_falls_back_to_cpu_when_dmabuf_import_fails` — the exported
//!   DMA-BUF fails to build (`DmabufBuildFailed`), the widget reports the
//!   presentation unhealthy, and the renderer switches to CPU readback: the
//!   widget ends up showing a `GdkMemoryTexture` (`render_surface.zig:156-171`,
//!   `OpenGL.zig:300-319`). MEASURED (rvm, GTK 4.22.4): GDK >= 4.14 imports a
//!   linear DMA-BUF on every backend, broadway included (it downloads it on the
//!   CPU), so R2's "broadway has no DMA-BUF import" does not hold and the failure
//!   is injected: the `render-surface-fault-inject` feature makes GDK itself
//!   reject the texture. Run under broadway (the brief's lane) or any display.
//! * `render_surface_presents_dmabuf_on_wayland` — on a Wayland display whose
//!   GDK imports DMA-BUFs, the widget shows a `GdkDmabufTexture`
//!   (`RUSTTY_REQUIRE_DMABUF` pivot: without an import path it skips, or fails
//!   under the pivot).
//! * `fractional_scale_cells_land_on_device_pixels` — on a 1.25x Wayland output
//!   (a headless sway, `swaymsg output HEADLESS-1 scale 1.25`), with the terminal
//!   placed at a fractional device origin, a column of full-block glyphs lands on
//!   device pixels: its edge column is fully opaque and there is no half-covered
//!   seam row above or below it (`scale.zig`, `render_surface.zig:176-205`). The
//!   output is captured with `grim` (a VM-side capture of a headless compositor).
//!
//! * `surface_init_failure_shows_the_error_page` — UD356 (below; since P8-W6 T2
//!   triggered by a per-surface context failure, `MESA_GL_VERSION_OVERRIDE=4.1`).
//!
//! * `render_surface_dmabuf_build_failure_skips_the_destroy_notify` — GDK never
//!   calls the DMA-BUF destroy notify for a build that failed, so the failure
//!   arm's own free of the planes' holder is the only one.
//!
//! Each test stands up GTK and a `GApplication` singleton, so each must run in its
//! own process: select one with its exact name.
//!
//!   GDK_BACKEND=broadway LIBGL_ALWAYS_SOFTWARE=1 cargo test -p rustty-apprt \
//!     --features gtk,render-surface-fault-inject --test gtk_render_surface -- \
//!     --ignored --exact render_surface_falls_back_to_cpu_when_dmabuf_import_fails
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
#![allow(non_snake_case)]

use std::io::Write as _;
use std::os::fd::{FromRawFd as _, RawFd};
use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;
use gio::prelude::*;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::render_surface::RenderSurface;
use rustty_apprt::gtk::class::surface::{CoreSurface, Surface, SurfaceOverrides};
use rustty_apprt::gtk::{self, register_resources, scale};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;

#[path = "common/gtk_live.rs"]
mod gtk_live;

/// The live app, its GTK half and the terminal widget, kept alive for the test
/// until `finish` closes the terminal and tears the apps down.
///
/// `render_threads` is the FIRST field. Struct fields drop in declaration order,
/// so if an assertion unwinds past `finish`, the guard stops the render thread
/// while `core_app` is still alive (P7-W7 T6, N450).
#[allow(dead_code)]
struct Live {
    render_threads: gtk_live::RenderThreadGuard,
    core_app: Box<CoreApp<gtk::App>>,
    core_ptr: *mut CoreApp<gtk::App>,
    /// Boxed: the application's `rt_app` backref is this address, and `Live` is
    /// moved out of `boot` after the bind (P7-W7 T6: an inline `gtk::App` left
    /// `rt_app` dangling, which `deleteSurface`'s `quit_timer` action read once
    /// `finish` began closing the surface).
    app: Box<gtk::App>,
    application: Application,
    window: libadwaita::ApplicationWindow,
    surface: Surface,
    render_surface: RenderSurface,
    core: *mut CoreSurface,
}

/// Stand up the core app, the GTK app and one `/bin/sh` terminal in a plain
/// window (the `gtk_first_light` scaffolding), with `margin` CSS px around the
/// surface, and pump until its core surface exists.
fn boot(margin: i32) -> Live {
    rustty_apprt::init().expect("initialize rustty global state");

    let mut core_app: Box<CoreApp<gtk::App>> =
        CoreApp::<gtk::App>::create(Global).expect("core App::create");
    let core_ptr: *mut CoreApp<gtk::App> = &mut *core_app;
    let app = Box::new(match gtk::App::init(core_ptr.cast()) {
        Ok(rt) => rt,
        Err(err) => match err {},
    });
    let application: Application = app.application().clone();
    application.upcast_ref::<gio::Application>().set_default();
    application
        .register(gio::Cancellable::NONE)
        .expect("register the application");
    // The boxed app's address, stable across the move of `Live` out of `boot`.
    let app_ptr: *mut std::ffi::c_void = std::ptr::from_ref::<gtk::App>(&app).cast_mut().cast();
    application.bind_rt_app(app_ptr);
    application.bind_core_app::<gtk::App>();
    register_resources();

    let surface = Surface::new(SurfaceOverrides {
        command: Some(Command::Direct(vec![b"/bin/sh".to_vec()])),
        shell_integration: None,
        working_directory: None,
        title: None,
    });
    surface.set_margin_top(margin);
    surface.set_margin_start(margin);
    let window = libadwaita::ApplicationWindow::new(&application);
    window.set_default_size(640, 400);
    window.set_content(Some(&surface));
    let render_surface = gtk_live::find_render_surface(surface.upcast_ref::<gtk4::Widget>())
        .expect("the surface template contains a RenderSurface");
    window.present();

    // Declared after `core_app` and `app`, so an unwind out of `boot` drops it
    // first (P7-W7 T6, N450).
    let render_threads = gtk_live::RenderThreadGuard::new(core_ptr, [&surface]);

    let ctx = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
        gtk_live::iterate(&ctx, core_ptr, &app);
        assert!(
            Instant::now() < deadline,
            "core surface was never built; is LIBGL_ALWAYS_SOFTWARE=1 set?"
        );
    }
    let core = surface.core().expect("core surface");
    Live {
        render_threads,
        core_app,
        core_ptr,
        app,
        application,
        window,
        surface,
        render_surface,
        core,
    }
}

/// Keep the main loop turning (a blocking iteration returns at least every 50 ms).
fn keepalive() -> glib::SourceId {
    glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue)
}

/// Pump (and tick the core app) until `pred` holds or `secs` pass; returns
/// whether it held.
fn pump_until(live: &Live, secs: u64, mut pred: impl FnMut() -> bool) -> bool {
    let ctx = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if pred() {
            return true;
        }
        gtk_live::iterate(&ctx, live.core_ptr, &live.app);
    }
    pred()
}

/// The pty master fd of the live `/bin/sh`.
///
/// # Safety
/// `core` is a live, initialized core surface with a started `Exec` backend.
unsafe fn pty_master(core: *mut CoreSurface) -> RawFd {
    match unsafe { &(*(*core).io.as_ptr()).backend } {
        rustty_termio::Backend::Exec(exec) => {
            exec.subprocess.pty.as_ref().expect("pty started").master
        }
    }
}

/// Write `bytes` into the shell's pty.
fn type_into_shell(live: &Live, bytes: &[u8]) {
    // SAFETY: `live.core` is the live core surface.
    let master = unsafe { pty_master(live.core) };
    let mut pipe = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(master) });
    pipe.write_all(bytes).expect("write into the pty");
    pipe.flush().ok();
}

/// Close the terminal the way the app does, then tear the apps down in
/// `run_gtk`'s order (`gtk_first_light.rs` section 7). `close_surface` asserts
/// that the `Surface` was finalized and its shell reaped. (This used to
/// `forget` the live objects, to dodge a termio `Subprocess` double-free that
/// has since been fixed; P7-W7 T6.)
fn finish(live: Live, keepalive: glib::SourceId) {
    keepalive.remove();
    let Live {
        render_threads,
        core_app,
        app,
        application,
        window,
        surface,
        render_surface,
        core,
        ..
    } = live;
    // Re-bound after the others: locals drop in reverse declaration order, so an
    // assertion that unwinds out of the close below drops the guard first, while
    // `core_app` is still alive.
    let render_threads = render_threads;
    // SAFETY: `core` is the live core surface; its shell was started by init.
    let shell_pid = unsafe { gtk_live::shell_pid(core) };
    drop(render_surface);
    drop(application);
    gtk_live::close_surface(&glib::MainContext::default(), window, surface, shell_pid);
    gtk_live::close_other_windows(&glib::MainContext::default());
    drop(render_threads);
    app.terminate();
    core_app.destroy();
}

/// Whether the surfaceless EGL display the renderer uses can export DMA-BUFs
/// (`EGL_MESA_image_dma_buf_export`), i.e. whether the renderer's first frames
/// are DMA-BUFs.
fn renderer_exports_dmabuf() -> bool {
    // The app-scoped render device the renderer's contexts are made on
    // (`40d5b860d`); the export entry points and the extension are its.
    let mut slot = std::mem::MaybeUninit::<rustty_renderer::Device>::uninit();
    match rustty_renderer::Device::init(&mut slot) {
        Ok(()) => {
            // SAFETY: `init` returned Ok, so it wrote the slot.
            let mut device = unsafe { slot.assume_init() };
            let exports = device.has_dmabuf_export();
            device.deinit();
            exports
        }
        Err(_) => false,
    }
}

/// `RUSTTY_REQUIRE_DMABUF=1` turns a DMA-BUF skip into a failure (R14).
fn require_dmabuf_or_skip(test: &str, why: &str) {
    if std::env::var_os("RUSTTY_REQUIRE_DMABUF").is_some_and(|v| v == "1") {
        panic!("{test}: RUSTTY_REQUIRE_DMABUF=1 but {why}");
    }
    eprintln!("{why}; skipping {test}");
}

#[test]
#[ignore = "headed GTK (broadway) with --features render-surface-fault-inject; run alone with --exact"]
fn render_surface_falls_back_to_cpu_when_dmabuf_import_fails() {
    const TEST: &str = "render_surface_falls_back_to_cpu_when_dmabuf_import_fails";
    #[cfg(not(feature = "render-surface-fault-inject"))]
    panic!("{TEST}: build with --features render-surface-fault-inject (see the module doc)");
    #[cfg(feature = "render-surface-fault-inject")]
    render_surface_falls_back_to_cpu_when_dmabuf_import_fails_body(TEST);
}

#[cfg(feature = "render-surface-fault-inject")]
fn render_surface_falls_back_to_cpu_when_dmabuf_import_fails_body(TEST: &str) {
    use rustty_apprt::gtk::class::render_surface::FORCE_DMABUF_IMPORT_FAILURE;
    assert!(
        renderer_exports_dmabuf(),
        "{TEST}: the renderer's EGL display cannot export DMA-BUFs, so this would not \
         exercise the import failure (the rvm llvmpipe display can)"
    );
    FORCE_DMABUF_IMPORT_FAILURE.store(true, std::sync::atomic::Ordering::SeqCst);
    let live = boot(0);
    let ka = keepalive();

    let display = gtk4::gdk::Display::default().expect("a GDK display");
    eprintln!("{TEST}: display {}", display.type_().name());

    // The renderer exports a DMA-BUF, GDK cannot build a texture from it, the
    // widget reports the presentation unhealthy, and the next frames arrive as
    // CPU memory: the widget ends up showing a memory texture with pixels.
    let rs = live.render_surface.clone();
    let fell_back = pump_until(&live, 30, || {
        rs.texture().is_some_and(|t| {
            gtk_live::texture_kind(&t) == "memory"
                && gtk_live::nonzero_bytes(&gtk_live::texture_rgba(&t).2) > 0
        })
    });
    let kind = rs.texture().map(|t| gtk_live::texture_kind(&t));
    assert!(
        fell_back,
        "the RenderSurface never showed a CPU-readback texture (last: {kind:?})"
    );
    eprintln!("{TEST}: broadway fell back to a memory texture");
    drop(rs); // the close below asserts the RenderSurface is finalized
    finish(live, ka);
}

/// The failure arm of `rebuild_dmabuf_texture` frees the planes' holder itself,
/// which is sound only if GDK does not ALSO call the destroy notify for a build
/// that failed (a double free). On Wayland: first let DMA-BUF textures build and
/// be replaced (so GDK's notify demonstrably fires for built textures), then
/// force the import failure and check that GDK never calls the notify for the
/// failed builds.
#[test]
#[ignore = "headed GTK on a Wayland display with DMA-BUF import and --features render-surface-fault-inject; run alone with --exact"]
fn render_surface_dmabuf_build_failure_skips_the_destroy_notify() {
    const TEST: &str = "render_surface_dmabuf_build_failure_skips_the_destroy_notify";
    #[cfg(not(feature = "render-surface-fault-inject"))]
    panic!("{TEST}: build with --features render-surface-fault-inject (see the module doc)");
    #[cfg(feature = "render-surface-fault-inject")]
    render_surface_dmabuf_build_failure_skips_the_destroy_notify_body(TEST);
}

#[cfg(feature = "render-surface-fault-inject")]
fn render_surface_dmabuf_build_failure_skips_the_destroy_notify_body(TEST: &str) {
    use rustty_apprt::gtk::class::render_surface::{dmabuf_witness, FORCE_DMABUF_IMPORT_FAILURE};
    use std::sync::atomic::Ordering::SeqCst;
    if !renderer_exports_dmabuf() {
        require_dmabuf_or_skip(TEST, "the renderer's EGL display exports no DMA-BUF");
        return;
    }
    let live = boot(0);
    let ka = keepalive();
    let rs = live.render_surface.clone();

    // Built DMA-BUF textures, each released when the next frame replaces it.
    let cycled = pump_until(&live, 30, || dmabuf_witness::DESTROY_CALLS.load(SeqCst) > 0);
    let kind = rs.texture().map(|t| gtk_live::texture_kind(&t));
    if !cycled {
        require_dmabuf_or_skip(
            TEST,
            &format!("no DMA-BUF texture was built and released (last texture: {kind:?})"),
        );
        drop(rs);
        finish(live, ka);
        return;
    }
    eprintln!(
        "{TEST}: {} built DMA-BUF texture(s) released through the notify",
        dmabuf_witness::DESTROY_CALLS.load(SeqCst)
    );

    // Now every build fails. Keep the frames coming (the terminal redraws on
    // output) and let GDK settle.
    FORCE_DMABUF_IMPORT_FAILURE.store(true, SeqCst);
    type_into_shell(&live, b"echo failure\n");
    let failed = pump_until(&live, 30, || {
        dmabuf_witness::BUILD_FAILURES.load(SeqCst) > 0
    });
    assert!(
        failed,
        "{TEST}: no DMA-BUF build failed after forcing the failure"
    );
    pump_until(&live, 2, || false);
    // The replaced texture is dropped when the widget goes away.
    live.window.close();
    pump_until(&live, 1, || false);

    assert_eq!(
        dmabuf_witness::DESTROY_CALLS_FOR_FAILED_BUILDS.load(SeqCst),
        0,
        "{TEST}: GDK called the destroy notify for {} failed build(s); the failure \
         arm's own free would then be a double free",
        dmabuf_witness::BUILD_FAILURES.load(SeqCst)
    );
    eprintln!(
        "{TEST}: {} failed build(s), 0 notifies for them",
        dmabuf_witness::BUILD_FAILURES.load(SeqCst)
    );
    drop(rs); // the close below asserts the RenderSurface is finalized
    finish(live, ka);
}

#[test]
#[ignore = "headed GTK on a Wayland display with DMA-BUF import; run alone with --exact"]
fn render_surface_presents_dmabuf_on_wayland() {
    const TEST: &str = "render_surface_presents_dmabuf_on_wayland";
    if !renderer_exports_dmabuf() {
        require_dmabuf_or_skip(TEST, "the renderer's EGL display exports no DMA-BUF");
        return;
    }
    let live = boot(0);
    let ka = keepalive();

    let display = gtk4::gdk::Display::default().expect("a GDK display");
    assert_eq!(
        display.type_().name(),
        "GdkWaylandDisplay",
        "{TEST} must run on a Wayland display"
    );

    let rs = live.render_surface.clone();
    let shown = pump_until(&live, 30, || {
        rs.texture()
            .is_some_and(|t| gtk_live::nonzero_bytes(&gtk_live::texture_rgba(&t).2) > 0)
    });
    assert!(shown, "the RenderSurface never showed a frame");
    // Let a few more frames through: a failed import would switch to memory.
    pump_until(&live, 2, || false);
    let texture = rs.texture().expect("a texture");
    let kind = gtk_live::texture_kind(&texture);
    eprintln!("{TEST}: the RenderSurface shows a {kind} texture");
    if kind != "dmabuf" {
        require_dmabuf_or_skip(
            TEST,
            &format!("GDK on this display did not import the DMA-BUF ({kind} texture)"),
        );
        drop(rs);
        finish(live, ka);
        return;
    }
    let (w, h, _) = gtk_live::texture_rgba(&texture);
    let device = rs.device_size();
    assert_eq!((w as u32, h as u32), (device.width, device.height));
    drop(rs); // the close below asserts the RenderSurface is finalized
    finish(live, ka);
}

/// UD356: when the renderer cannot initialize (here: the surface's own OpenGL
/// 4.3 core context cannot be created), `initSurface` fails and the surface
/// shows its error page. At `12752b2ac` nothing calls `setError(true)` any more
/// (`2f0b65346` removed the GLArea check that did), so the reference leaves the
/// surface blank; still so at `a60e9e2a5`.
///
/// P8-W6 T2 (`40d5b860d`, DECISIONS M4): the trigger moved. With no EGL vendor
/// at all (the old `__EGL_VENDOR_LIBRARY_FILENAMES=/nonexistent.json`) the app's
/// render device now fails in `App.init` (`CreateError::DeviceFailed`), so no
/// window or surface ever exists.
/// What still reaches `initSurface`'s catch arm is a per-surface failure after
/// the device exists: `MESA_GL_VERSION_OVERRIDE=4.1` caps Mesa below the 4.3 core
/// context each renderer creates (`EGL_BAD_MATCH`), while the device (display +
/// config) initializes. MEASURED on rvm (llvmpipe).
///
///   LIBGL_ALWAYS_SOFTWARE=1 MESA_GL_VERSION_OVERRIDE=4.1 cargo test ... --exact \
///     surface_init_failure_shows_the_error_page
#[test]
#[ignore = "headed GTK with MESA_GL_VERSION_OVERRIDE=4.1; run alone with --exact"]
fn surface_init_failure_shows_the_error_page() {
    const TEST: &str = "surface_init_failure_shows_the_error_page";
    assert!(
        std::env::var_os("MESA_GL_VERSION_OVERRIDE")
            .is_some_and(|v| v.to_string_lossy().starts_with("4.1")),
        "{TEST}: run with MESA_GL_VERSION_OVERRIDE=4.1 (a per-surface context failure)"
    );
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
    window.set_default_size(640, 400);
    window.set_content(Some(&surface));
    window.present();

    let ctx = glib::MainContext::default();
    let ka = keepalive();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !surface.error() && Instant::now() < deadline {
        gtk_live::iterate(&ctx, core_ptr, &app);
    }
    assert!(
        surface.core().is_none(),
        "{TEST}: the core surface must fail to initialize without a 4.3 core context"
    );
    assert!(
        surface.error(),
        "{TEST}: a surface whose initialization failed shows the error page (UD356)"
    );
    // The error page replaces the terminal as the surface's child (on an idle
    // rechild, `propError`).
    let deadline = Instant::now() + Duration::from_secs(5);
    let shown = |s: &Surface| {
        s.child()
            .is_some_and(|c| c.type_().name() == "AdwStatusPage")
    };
    while !shown(&surface) && Instant::now() < deadline {
        gtk_live::iterate(&ctx, core_ptr, &app);
    }
    assert!(
        shown(&surface),
        "{TEST}: the error page is the surface's child"
    );
    ka.remove();
    std::mem::forget((window, surface, application, app, core_app));
}

/// A `grim -t ppm -` capture of the whole output, as tight RGBA rows.
fn grim_capture() -> (usize, usize, Vec<u8>) {
    let out = std::process::Command::new("grim")
        .args(["-t", "ppm", "-"])
        .output()
        .expect("run grim (a VM-side capture of the headless compositor)");
    assert!(out.status.success(), "grim failed: {:?}", out);
    let data = out.stdout;
    // P6\n<w> <h>\n255\n<rgb...>
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while data[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while !data[i].is_ascii_whitespace() {
            i += 1;
        }
        fields.push(String::from_utf8_lossy(&data[start..i]).into_owned());
    }
    i += 1; // the single whitespace after maxval
    assert_eq!(fields[0], "P6");
    let w: usize = fields[1].parse().expect("ppm width");
    let h: usize = fields[2].parse().expect("ppm height");
    let rgb = &data[i..i + w * h * 3];
    let mut rgba = Vec::with_capacity(w * h * 4);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    (w, h, rgba)
}

#[test]
#[ignore = "headed GTK on a 1.25x Wayland output (headless sway) with grim; run alone with --exact"]
fn fractional_scale_cells_land_on_device_pixels() {
    const TEST: &str = "fractional_scale_cells_land_on_device_pixels";
    // 47 CSS px of margin puts the terminal at a fractional device origin at
    // 1.25x (47 * 1.25 = 58.75), as tab chrome does.
    let live = boot(47);
    let ka = keepalive();

    let rs = live.render_surface.clone();
    let widget = rs.upcast_ref::<gtk4::Widget>().clone();
    assert!(
        pump_until(&live, 20, || rs.texture().is_some()),
        "the RenderSurface never showed a frame"
    );
    let layout = scale::widget_layout(&widget);
    assert_eq!(layout.scale, 1.25, "{TEST} must run on a 1.25x output");
    let device_y = layout.origin.y * layout.scale;
    let device_x = layout.origin.x * layout.scale;
    eprintln!(
        "{TEST}: surface origin css=({}, {}) device=({device_x}, {device_y}) snap={:?}",
        layout.origin.x,
        layout.origin.y,
        layout.snap_origin()
    );
    assert!(
        device_y.fract() != 0.0 && device_x.fract() != 0.0,
        "{TEST}: the terminal must sit at a fractional device origin to test snapping"
    );

    // Magenta full blocks on three lines.
    type_into_shell(
        &live,
        b"clear; printf '\\033[38;2;255;0;255m%s\\033[0m\\n' \
          '\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88' \
          '\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88' \
          '\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88\xe2\x96\x88'\n",
    );
    pump_until(&live, 3, || false);

    let is_magenta = |p: &[u8]| p[0] >= 247 && p[1] <= 8 && p[2] >= 247;
    let tinted = |p: &[u8]| p[0] > 64 && p[2] > 64 && p[1] < 200;

    let (w, h, px) = grim_capture();
    let at = |x: usize, y: usize| &px[(y * w + x) * 4..(y * w + x) * 4 + 4];
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if is_magenta(at(x, y)) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    assert!(x0 != usize::MAX, "{TEST}: no magenta block on the output");
    let dump = std::env::var("RUSTTY_DUMP_PPM")
        .unwrap_or_else(|_| "/tmp/rustty-fractional-scale.ppm".to_string());
    gtk_live::write_ppm(&dump, &px, w, h);
    eprintln!("{TEST}: blocks at device x={x0}..={x1} y={y0}..={y1}; capture {dump}");

    // The block's left edge column is fully opaque on every row of the block.
    let edge_bad: Vec<usize> = (y0..=y1).filter(|&y| !is_magenta(at(x0, y))).collect();
    // No half-covered seam row just above or below the block.
    let seam = |y: usize| {
        (x0..=x1)
            .filter(|&x| tinted(at(x, y)) && !is_magenta(at(x, y)))
            .count()
    };
    let above = if y0 > 0 { seam(y0 - 1) } else { 0 };
    let below = if y1 + 1 < h { seam(y1 + 1) } else { 0 };
    // ...nor a half-covered column just left of it.
    let left = if x0 > 0 {
        (y0..=y1)
            .filter(|&y| tinted(at(x0 - 1, y)) && !is_magenta(at(x0 - 1, y)))
            .count()
    } else {
        0
    };
    eprintln!(
        "{TEST}: edge rows not opaque={}, seam px above={above} below={below}, left={left}",
        edge_bad.len()
    );
    assert!(
        edge_bad.is_empty() && above == 0 && below == 0 && left == 0,
        "{TEST}: glyph edges are not on device pixels (edge rows not opaque: {:?}; \
         half-covered px above={above} below={below} left={left})",
        edge_bad
    );
    drop((widget, rs)); // the close below asserts the RenderSurface is finalized
    finish(live, ka);
}
