//! GLYPH-DUMP — a validation harness (NOT a CI KAT). Clones the `gtk_first_light`
//! setup but writes some visible text into the live shell, lets the renderer draw the
//! grid, then reads GDK's GLArea framebuffer back and dumps it to a PPM (path from the
//! `RUSTTY_DUMP_PPM` env var). This exists to get a GPU-accurate screenshot of rustty's
//! REAL rendered output on machines where the compositor screenshot path cannot read a
//! GTK4 GLArea (e.g. `import`/XGetImage under XWayland captures the GLArea blank).
//!
//! Run:  RUSTTY_DUMP_PPM=/path/out.ppm \
//!         cargo test -p rustty-apprt --features gtk --test gtk_glyph_dump -- --ignored --exact glyph_dump
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")` — compiles out off the gate.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
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
const NEEDLE: &[u8] = b"RUSTTY-GLYPH-DUMP-OK";

#[derive(Default)]
struct Dump {
    captured: Cell<bool>,
    core_present: Cell<bool>,
    draw_ok: Cell<bool>,
    draw_err: RefCell<Option<String>>,
    pixels_len: Cell<usize>,
    pixels_nonzero: Cell<usize>,
    /// The raw RGBA readback (bottom-up, as glReadPixels returns it).
    rgba: RefCell<Vec<u8>>,
    dims: Cell<(usize, usize)>,
}

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

unsafe fn draw_present_read_glarea_fb(core: *mut CoreSurface, out: &Dump) {
    unsafe {
        let draw = rustty_core::renderer_seam::Renderer::drawFrame(&raw mut (*core).renderer, true);
        out.draw_ok.set(draw.is_ok());
        *out.draw_err.borrow_mut() = draw.err().map(|e| format!("{e:?}"));

        let renderer = &(*core).renderer.0;
        let w = renderer.size.screen.width as usize;
        let h = renderer.size.screen.height as usize;
        if w > 0 && h > 0 {
            let px = renderer.api.read_bound_framebuffer_rgba(w, h);
            out.pixels_len.set(px.len());
            out.pixels_nonzero
                .set(px.iter().filter(|&&b| b != 0).count());
            out.dims.set((w, h));
            *out.rgba.borrow_mut() = px;
        }
    }
}

/// Write the captured RGBA (bottom-up) to a top-down binary PPM (P6, RGB).
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

    let outcome = Rc::new(Dump::default());
    let gl_area = find_gl_area(surface.upcast_ref::<gtk4::Widget>())
        .expect("the surface template contains a GLArea");

    window.present();

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

    // Pump until the lazy core surface exists.
    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
        ctx.iteration(true);
        if Instant::now() > deadline {
            break;
        }
    }
    let core = surface
        .core()
        .expect("core surface built on first GLArea resize");

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
        ctx.iteration(true);
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

    // ---- Now let the renderer draw the grid WITH the text, then capture. ----
    let capture_deadline = Instant::now() + Duration::from_secs(10);
    while !outcome.captured.get() && Instant::now() < capture_deadline {
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
        outcome.core_present.set(true);
        unsafe { draw_present_read_glarea_fb(core, &outcome) };
        if outcome.pixels_nonzero.get() > 0 {
            outcome.captured.set(true);
        }
    }

    assert!(outcome.captured.get(), "captured a non-zero GLArea frame");
    let (w, h) = outcome.dims.get();
    assert!(w > 0 && h > 0, "the GLArea viewport was sized");
    write_ppm(&ppm_path, &outcome.rgba.borrow(), w, h);
    eprintln!(
        "GLYPH-DUMP: wrote {ppm_path} ({w}x{h}), {} of {} bytes non-zero",
        outcome.pixels_nonzero.get(),
        outcome.pixels_len.get()
    );

    // Clean teardown (mirrors first_light).
    keepalive.remove();
    render_pump.remove();
    drop(gl_area);
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
