//! GLYPH-DUMP — a validation harness (NOT a CI KAT). Clones the `gtk_first_light`
//! setup but writes some visible text into the live shell, lets the renderer draw the
//! grid, then downloads the `GdkTexture` the `RenderSurface` built from the renderer's
//! exported frame and dumps it to a PPM (path from the `RUSTTY_DUMP_PPM` env var). This
//! exists to get a GPU-accurate image of rustty's REAL rendered output without a
//! compositor screenshot. (P7-W7 T2, `2f0b65346`: it read GDK's GLArea framebuffer
//! back before the GLArea was replaced.)
//!
//! Run:  RUSTTY_DUMP_PPM=/path/out.ppm \
//!         cargo test -p rustty-apprt --features gtk --test gtk_glyph_dump -- --ignored --exact glyph_dump
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")` — compiles out off the gate.
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
use rustty_apprt::gtk::class::surface::{CoreSurface, Surface, SurfaceOverrides};
use rustty_apprt::gtk::{self, register_resources};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;

#[path = "common/gtk_live.rs"]
mod gtk_live;

/// The marker the live `/bin/sh` echoes back through the pty into the `Screen`.
const NEEDLE: &[u8] = b"RUSTTY-GLYPH-DUMP-OK";

unsafe fn screen_dump(core: *mut CoreSurface) -> Vec<u8> {
    unsafe {
        let mutex = (*core).renderer_state.mutex;
        (*mutex).lock();
        let active = (*(*core).renderer_state.terminal).screens.active;
        let dump = (*active).dump_string_alloc(
            Global.into(),
            rustty_vt::point::Point::Active(rustty_vt::point::Coordinate::default()),
        );
        (*mutex).unlock();
        dump.unwrap_or_default()
    }
}

unsafe fn pty_master(core: *mut CoreSurface) -> RawFd {
    match unsafe { &(*core).io.backend } {
        rustty_termio::Backend::Exec(exec) => {
            exec.subprocess.pty.as_ref().expect("pty started").master
        }
    }
}

#[test]
#[ignore = "headed GTK + GL 4.3-core context + a live shell; run with \
            RUSTTY_DUMP_PPM=/path/out.ppm cargo test -p rustty-apprt --features gtk \
            --test gtk_glyph_dump -- --ignored --exact glyph_dump"]
fn glyph_dump() {
    let ppm_path = std::env::var("RUSTTY_DUMP_PPM")
        .unwrap_or_else(|_| "/tmp/rustty-glyph-dump.ppm".to_string());

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

    let render_surface = gtk_live::find_render_surface(surface.upcast_ref::<gtk4::Widget>())
        .expect("the surface template contains a RenderSurface");

    window.present();

    let ctx = glib::MainContext::default();
    let keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);

    // Pump until the lazy core surface exists.
    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
        gtk_live::iterate(&ctx, core_ptr, &app);
        if Instant::now() > deadline {
            break;
        }
    }
    let core = surface
        .core()
        .expect("core surface built on first RenderSurface resize");

    // ---- Write visible text into the live shell FIRST, so the captured frame shows
    //      real rendered glyphs (not a bare prompt). ----
    {
        let master = unsafe { pty_master(core) };
        let mut pipe = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(master) });
        pipe.write_all(
            b"printf '%s\\n' 'RUSTTY-GLYPH-DUMP-OK' 'the quick brown fox jumps 0123456789' 'ABCDEFG abcdefg <>{}[]()#@&*'\n",
        )
        .expect("write the print command into the pty");
        pipe.flush().ok();
    }

    // Wait for the text to round-trip into the Screen grid.
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
        "shell text did not round-trip; screen saw: {:?}",
        String::from_utf8_lossy(&unsafe { screen_dump(core) })
    );

    // ---- Now let the renderer draw the grid WITH the text, then capture the
    //      texture the RenderSurface built from the latest exported frame. ----
    let settle = Instant::now() + Duration::from_millis(800);
    while Instant::now() < settle {
        gtk_live::iterate(&ctx, core_ptr, &app);
    }
    let capture_deadline = Instant::now() + Duration::from_secs(10);
    let mut captured = None;
    while captured.is_none() && Instant::now() < capture_deadline {
        gtk_live::iterate(&ctx, core_ptr, &app);
        if let Some(texture) = render_surface.texture() {
            let (w, h, px) = gtk_live::texture_rgba(&texture);
            if gtk_live::nonzero_bytes(&px) > 0 {
                captured = Some((w, h, px, gtk_live::texture_kind(&texture)));
            }
        }
    }
    let (w, h, mut rgba, kind) = captured.expect("captured a non-zero RenderSurface texture");
    assert!(w > 0 && h > 0, "the texture has an extent");
    // The OpenGL frame's rows are bottom-up (the widget flips it when drawing,
    // `render_surface.zig:188-199`); flip for a normal image.
    if !rustty_renderer::generic::CUSTOM_SHADER_Y_IS_DOWN {
        let row = w * 4;
        let mut flipped = Vec::with_capacity(rgba.len());
        for y in (0..h).rev() {
            flipped.extend_from_slice(&rgba[y * row..(y + 1) * row]);
        }
        rgba = flipped;
    }
    gtk_live::write_ppm(&ppm_path, &rgba, w, h);
    eprintln!(
        "GLYPH-DUMP: wrote {ppm_path} ({w}x{h}, {kind} texture), {} of {} bytes non-zero",
        gtk_live::nonzero_bytes(&rgba),
        rgba.len()
    );

    // Clean teardown (mirrors first_light).
    keepalive.remove();
    drop(render_surface);
    // SAFETY: `core` is the live core surface; stopped once, after the last frame.
    unsafe { gtk_live::stop_render_thread(core) };
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
        "App::run returns Ok: {:?}",
        run_result.err()
    );
}
