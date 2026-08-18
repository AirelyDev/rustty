//! CLIPBOARD KAT — exercises the copy (set_clipboard) write path end-to-end against a
//! real GDK clipboard, and the paste format-gate. The full paste→core delivery
//! (completeClipboardRequest → pty) is verified by running the app manually.
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;
use gio::prelude::*;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::surface::{Surface, SurfaceOverrides};
use rustty_apprt::gtk::{self, register_resources};
use rustty_apprt_api::structs::{Clipboard, ClipboardContent, ClipboardRequest};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;

#[test]
#[ignore = "headed GTK; run: cargo test -p rustty-apprt --features gtk \
            --test gtk_clipboard -- --ignored --exact clipboard_copy_roundtrips_and_paste_gate"]
fn clipboard_copy_roundtrips_and_paste_gate() {
    // Force the X11 backend: under Wayland an unfocused/unactivated window cannot claim
    // the clipboard selection, so a headless test can't observe the write. XWayland has
    // no such focus gate, so the copy write round-trips. Must be set before GTK inits.
    // SAFETY: single-threaded, before any GTK/threads start.
    unsafe { std::env::set_var("GDK_BACKEND", "x11") };

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
    let surface_widget = surface.upcast_ref::<gtk4::Widget>().clone();
    window.present();

    let ctx = glib::MainContext::default();
    // Pump until realized (the GLArea gets a display / clipboard).
    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
        ctx.iteration(true);
        if Instant::now() > deadline {
            break;
        }
    }
    for _ in 0..20 {
        ctx.iteration(true);
    }

    // ---- Copy: write via set_clipboard, then read the SAME clipboard back. ----
    const NEEDLE: &str = "RUSTTY-CLIP-ROUNDTRIP-42 áé <>{}";
    surface.set_clipboard(
        Clipboard::Standard,
        &[ClipboardContent {
            mime: b"text/plain",
            data: NEEDLE.as_bytes(),
        }],
        false,
    );
    for _ in 0..10 {
        ctx.iteration(true);
    }

    let readback: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    {
        let readback = readback.clone();
        let clipboard = WidgetExt::clipboard(&surface_widget);
        clipboard.read_text_async(gio::Cancellable::NONE, move |res| {
            if let Ok(Some(t)) = res {
                *readback.borrow_mut() = Some(t.to_string());
            } else {
                *readback.borrow_mut() = Some(String::new());
            }
        });
    }
    let rt_deadline = Instant::now() + Duration::from_secs(10);
    while readback.borrow().is_none() && Instant::now() < rt_deadline {
        ctx.iteration(true);
    }
    assert_eq!(
        readback.borrow().as_deref(),
        Some(NEEDLE),
        "set_clipboard(text/plain) must round-trip through the GDK clipboard"
    );

    // ---- Paste gate: with text on the clipboard, a Paste request starts (Ok(true)). ----
    let started = surface
        .clipboard_request(Clipboard::Standard, ClipboardRequest::Paste)
        .expect("clipboard_request must not error");
    assert!(
        started,
        "clipboard_request(Paste) must return Ok(true) when the clipboard has text"
    );
    for _ in 0..10 {
        ctx.iteration(true);
    }

    {
        let app_weak = application.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(120), move || {
            if let Some(app) = app_weak.upgrade() {
                app.quit_now();
            }
        });
    }
    let _ = app.run();
}
