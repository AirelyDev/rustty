//! PARITY CALLBACK-FIRING KAT — fires the newly-wired template callbacks and notify
//! handlers from the parity batch, to catch the recurring runtime-abort bug (a
//! non-swapped callback declared `&self` compiles but aborts with WrongValueType only
//! when the signal fires). Static review has missed this before; firing is the gate.
//!
//! Covers: mouse-shape/hidden/hover-url notify handlers; the SearchOverlay signals →
//! surface search_stop/changed/next/previous; the DropTarget `drop` (the former live
//! crash); and construction of the new CommandPalette / GlobalShortcuts classes.
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")`.
#![cfg(all(target_os = "linux", feature = "gtk"))]
#![allow(non_snake_case)]

use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;
use gio::prelude::*;
use glib::prelude::*;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::surface::{Surface, SurfaceOverrides};
use rustty_apprt::gtk::{self, register_resources};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;

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

/// Walk the widget tree and every widget's controllers to find a DropTarget.
fn find_drop_target(widget: &gtk4::Widget) -> Option<gtk4::DropTarget> {
    let controllers = widget.observe_controllers();
    for i in 0..controllers.n_items() {
        if let Some(obj) = controllers.item(i) {
            if let Ok(dt) = obj.downcast::<gtk4::DropTarget>() {
                return Some(dt);
            }
        }
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        if let Some(dt) = find_drop_target(&c) {
            return Some(dt);
        }
        child = c.next_sibling();
    }
    None
}

#[test]
#[ignore = "headed GTK; run: cargo test -p rustty-apprt --features gtk \
            --test gtk_parity_callbacks -- --ignored --exact parity_callbacks_do_not_abort"]
fn parity_callbacks_do_not_abort() {
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

    // No window/present/realize: the template children + controllers are bound at
    // construction, and the callbacks marshal args (the abort surface) BEFORE their
    // body checks `self.core()`. So firing them on the unrealized widget exercises the
    // exact signature/marshalling risk without the slow core-build + teardown.
    let surface = Surface::new(SurfaceOverrides {
        command: Some(Command::Direct(vec![b"/bin/sh".to_vec()])),
        working_directory: None,
        title: None,
    });
    let sw = surface.upcast_ref::<gtk4::Widget>().clone();
    let ctx = glib::MainContext::default();

    // (A) notify handlers: setting these properties emits notify::* → the surface's
    //     mouse-shape/hidden/hover-url template/notify callbacks. A mis-shaped one
    //     aborts here.
    surface.set_property("mouse-shape", 2i32);
    surface.set_property("mouse-hidden", true);
    surface.set_property("mouse-hidden", false);
    surface.set_property("mouse-hover-url", "https://example.com/rustty");
    surface.set_property("mouse-hover-url", None::<String>);
    for _ in 0..10 {
        ctx.iteration(true);
    }

    // (B) SearchOverlay signals → surface search_stop/changed/next/previous.
    let mut search = Vec::new();
    find_by_type(&sw, "GhosttySearchOverlay", &mut search);
    assert!(!search.is_empty(), "surface template has a SearchOverlay");
    for o in &search {
        o.emit_by_name::<()>("search-changed", &[&"needle"]);
        o.emit_by_name::<()>("next-match", &[]);
        o.emit_by_name::<()>("previous-match", &[]);
        o.emit_by_name::<()>("stop-search", &[]);
    }
    for _ in 0..10 {
        ctx.iteration(true);
    }

    // (C) DropTarget `drop` (the former live crash). Emit with a string Value → the
    //     string branch (paste). If the signature is wrong this aborts.
    if let Some(dt) = find_drop_target(&sw) {
        let val = "dropped-text".to_value();
        let _ = dt.emit_by_name::<bool>("drop", &[&val, &10.0f64, &10.0f64]);
        for _ in 0..10 {
            ctx.iteration(true);
        }
    } else {
        panic!("surface has no DropTarget to fire drop against");
    }

    // (D) New standalone classes construct (template load / class registration).
    {
        use rustty_apprt::gtk::class::command_palette::CommandPalette;
        use rustty_apprt::gtk::class::global_shortcuts::GlobalShortcuts;
        let _palette = CommandPalette::new();
        let _shortcuts = GlobalShortcuts::new();
    }

    // Reaching here means none of the fired callbacks aborted the process.
    let _ = (app, application, sw);
}
