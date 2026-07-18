//! INPUT REGRESSION KAT — the placeholder overlay widgets must NOT swallow input.
//!
//! Bug it guards (found by running the real binary): `Surface`'s template stacks the
//! T24 "inhabited placeholder" overlays (`GhosttyResizeOverlay`, `GhosttySearchOverlay`,
//! `GhosttyKeyStateOverlay`) on top of the GLArea via GtkOverlay. A bare `adw::Bin`
//! subclass defaults to `visible: true` + `can-target: true`, and an overlay child with
//! default FILL/FILL alignment is allocated the FULL terminal rectangle. So each empty
//! placeholder was an invisible full-size sheet of glass that rendered nothing but
//! swallowed every pointer event → the GLArea never got focus → NO keystrokes reached
//! the terminal. The reference keeps all three hidden by default in their own `.blp`
//! (`resize-overlay.blp: visible:false; can-target:false`, `search-overlay.blp:
//! visible: bind template.active`, `key-state-overlay.blp: visible: bind $has_state(...)`).
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")`.
#![cfg(all(target_os = "linux", feature = "gtk"))]
#![allow(non_snake_case)]

use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;
use gio::prelude::*;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::surface::{Surface, SurfaceOverrides};
use rustty_apprt::gtk::{self, register_resources};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;

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

/// Collect every widget in the tree whose GType name matches `type_name`.
fn find_by_type(widget: &gtk4::Widget, type_name: &str, out: &mut Vec<gtk4::Widget>) {
    if widget.type_().name() == type_name {
        out.push(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        find_by_type(&c, type_name, out);
        child = c.next_sibling();
    }
}

#[test]
#[ignore = "headed GTK; run: cargo test -p rustty-apprt --features gtk \
            --test gtk_input_focus -- --ignored --exact placeholder_overlays_do_not_capture_input"]
fn placeholder_overlays_do_not_capture_input() {
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
    let surface_widget = surface.upcast_ref::<gtk4::Widget>().clone();
    let gl_area = find_gl_area(&surface_widget).expect("the surface template contains a GLArea");
    window.present();

    let ctx = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    while surface.core().is_none() {
        ctx.iteration(true);
        if Instant::now() > deadline {
            break;
        }
    }
    for _ in 0..40 {
        ctx.iteration(true);
    }

    // (1) The three placeholder overlays must be hidden by default — otherwise they
    //     cover the GLArea and swallow input.
    for type_name in [
        "GhosttyResizeOverlay",
        "GhosttySearchOverlay",
        "GhosttyKeyStateOverlay",
    ] {
        let mut found = Vec::new();
        find_by_type(&surface_widget, type_name, &mut found);
        assert!(
            !found.is_empty(),
            "the surface template must instantiate {type_name}"
        );
        for w in &found {
            assert!(
                !w.is_visible(),
                "{type_name} must be hidden by default so it does not cover the GLArea \
                 and swallow pointer/keyboard input"
            );
        }
    }

    // (2) With the overlays out of the way, the owner-initiated focus path must land
    //     keyboard focus on the GLArea (the input part).
    surface.grab_focus();
    for _ in 0..10 {
        ctx.iteration(true);
    }
    assert!(
        gl_area.is_focus(),
        "surface.grab_focus() must focus the inner GLArea; a covering overlay would \
         prevent the GLArea from ever becoming the focus widget"
    );

    // (3) Fire the context-menu `closed` and child-exited `close-request` signals.
    //     These template callbacks were unimplemented!()/wrong-signature placeholders
    //     that ABORTED the process (non-unwinding panic across gtk4-rs's trampoline)
    //     on ordinary interaction — right-clicking then dismissing the menu, or
    //     clicking the child-exit banner's close button. If a callback's signature is
    //     wrong (e.g. `&self` instead of the non-swapped `(emitter, this)`), or its
    //     body is unimplemented, the trampoline aborts HERE and kills this test
    //     process — so reaching the end of the test is the pass condition.
    let mut popovers = Vec::new();
    find_by_type(&surface_widget, "GtkPopoverMenu", &mut popovers);
    assert!(
        !popovers.is_empty(),
        "the surface template must contain the context-menu PopoverMenu"
    );
    for p in &popovers {
        p.emit_by_name::<()>("closed", &[]);
    }
    let mut child_exits = Vec::new();
    find_by_type(&surface_widget, "GhosttySurfaceChildExited", &mut child_exits);
    assert!(
        !child_exits.is_empty(),
        "the surface template must contain the child-exited overlay"
    );
    for w in &child_exits {
        w.emit_by_name::<()>("close-request", &[]);
    }
    for _ in 0..10 {
        ctx.iteration(true);
    }

    {
        let app_weak = application.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(150), move || {
            if let Some(app) = app_weak.upgrade() {
                app.quit_now();
            }
        });
    }
    let _ = app.run();
}
