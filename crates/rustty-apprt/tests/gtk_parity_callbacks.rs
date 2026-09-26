//! PARITY CALLBACK-FIRING KAT — fires the newly-wired template callbacks and notify
//! handlers from the parity batch, to catch the recurring runtime-abort bug (a
//! non-swapped callback declared `&self` compiles but aborts with WrongValueType only
//! when the signal fires). Static review has missed this before; firing is the gate.
//!
//! Covers: mouse-shape/hidden/hover-url notify handlers; the SearchOverlay signals →
//! surface search_stop/changed/next/previous; the DropTarget `drop` (the former live
//! crash); and construction of the new CommandPalette / GlobalShortcuts classes.
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
#![allow(non_snake_case)]

use allocator_api2::alloc::Global;
use gio::prelude::*;
use gtk4::prelude::*;

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

/// Walk the widget tree and every widget's controllers to collect every DropTarget.
///
/// CORRECTION (task L-2): this returned the FIRST DropTarget only, which is the
/// surface's `surface_drop_target` (split drag-and-drop), not the paste
/// `drop_target` the (C) block below describes; both are now fired.
fn find_drop_targets(widget: &gtk4::Widget, out: &mut Vec<gtk4::DropTarget>) {
    let controllers = widget.observe_controllers();
    for i in 0..controllers.n_items() {
        if let Some(obj) = controllers.item(i) {
            if let Ok(dt) = obj.downcast::<gtk4::DropTarget>() {
                out.push(dt);
            }
        }
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        find_drop_targets(&c, out);
        child = c.next_sibling();
    }
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
        shell_integration: None,
        working_directory: None,
        title: None,
    });
    let sw = surface.upcast_ref::<gtk4::Widget>().clone();
    let ctx = glib::MainContext::default();

    // (A) notify handlers: setting these properties emits notify::* → the surface's
    //     mouse-shape/hidden/hover-url template/notify callbacks. A mis-shaped one
    //     aborts here.
    // CORRECTION (task L-2): this set the property from `2i32`, which GLib has
    // rejected since task 20 (S54-F3) typed `mouse-shape` as the
    // `GhosttyMouseShape` enum (`surface.zig:2264-2272`); the test panicked there.
    surface.set_property(
        "mouse-shape",
        rustty_apprt::gtk::class::gtypes::MouseShapeEnum::Pointer,
    );
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
    let mut drop_targets = Vec::new();
    find_drop_targets(&sw, &mut drop_targets);
    assert!(
        drop_targets.len() >= 2,
        "the surface has its paste and split drop targets; found {}",
        drop_targets.len()
    );
    for dt in &drop_targets {
        // CORRECTION (task L-2): GtkDropTarget's `drop` declares its first
        // argument as a boxed `GValue` (G_TYPE_VALUE); emitting the bare string
        // value panicked in glib-rs ("expected GValue, got gchararray") before any
        // handler ran, so the value is boxed as GTK itself passes it.
        let val = glib::value::BoxedValue("dropped-text".to_value());
        let _ = dt.emit_by_name::<bool>("drop", &[&val, &10.0f64, &10.0f64]);
        // Non-blocking: with both targets fired there may be nothing left to
        // dispatch, and a blocking iteration would then wait forever.
        for _ in 0..10 {
            ctx.iteration(false);
        }
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
