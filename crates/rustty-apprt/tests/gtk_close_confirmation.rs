//! T23 CLOSE-CONFIRMATION KAT — exercises the CloseConfirmationDialog at runtime.
//!
//! Compile-time green does NOT prove this works: the risky bits are all runtime —
//! (1) the CompositeTemplate loads from the gresource (`$GhosttyCloseConfirmationDialog:
//! $GhosttyDialog` requires the GhosttyDialog GType registered + the .ui in the bundle),
//! (2) the `response` vfunc override dispatches through the custom `Dialog` base
//! (a 2-level custom subclass of adw::AlertDialog), and (3) emitting a response maps to
//! the `close-request` / `cancel` signals. This test drives all three.
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")`.
#![cfg(all(target_os = "linux", feature = "gtk"))]
#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use allocator_api2::alloc::Global;
use gio::prelude::*;
use glib::prelude::*;
use libadwaita::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::close_confirmation_dialog::{CloseConfirmationDialog, Target};
use rustty_apprt::gtk::{self, register_resources};
use rustty_core::app::App as CoreApp;

#[test]
#[ignore = "headed GTK; run: cargo test -p rustty-apprt --features gtk \
            --test gtk_close_confirmation -- --ignored --exact close_confirmation_dialog_maps_responses"]
fn close_confirmation_dialog_maps_responses() {
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
    register_resources();

    // (1) Construction loads the CompositeTemplate from the gresource. If the .ui is
    //     missing from the bundle or GhosttyDialog isn't registered, this aborts here.
    let dialog = CloseConfirmationDialog::new(Target::Window);
    assert_eq!(
        dialog.target(),
        Target::Window,
        "the target property round-trips"
    );

    let close_fired = Rc::new(Cell::new(0u32));
    let cancel_fired = Rc::new(Cell::new(0u32));
    {
        let c = close_fired.clone();
        dialog.connect_local("close-request", false, move |_| {
            c.set(c.get() + 1);
            None
        });
        let x = cancel_fired.clone();
        dialog.connect_local("cancel", false, move |_| {
            x.set(x.get() + 1);
            None
        });
    }

    // (2)+(3) Emitting adw's `response` signal runs the class handler — our `response`
    //     vfunc override on the custom Dialog base — which must map "close" -> the
    //     `close-request` signal and anything else -> `cancel`.
    dialog.emit_by_name::<()>("response", &[&"close"]);
    assert_eq!(close_fired.get(), 1, "response \"close\" emits close-request");
    assert_eq!(cancel_fired.get(), 0, "response \"close\" does NOT emit cancel");

    dialog.emit_by_name::<()>("response", &[&"cancel"]);
    assert_eq!(cancel_fired.get(), 1, "response \"cancel\" emits cancel");
    assert_eq!(close_fired.get(), 1, "response \"cancel\" does NOT emit close-request");

    // (4) present(None) must not abort (sets heading/body from Target, then presents).
    dialog.present(None);
    let ctx = glib::MainContext::default();
    for _ in 0..10 {
        ctx.iteration(false);
    }
    dialog.close();

    // Also cover the other three targets construct + present without aborting.
    for t in [Target::App, Target::Tab, Target::Surface] {
        let d = CloseConfirmationDialog::new(t);
        assert_eq!(d.target(), t);
        d.present(None);
        for _ in 0..3 {
            ctx.iteration(false);
        }
        d.close();
    }

    {
        let app_weak = application.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(100), move || {
            if let Some(app) = app_weak.upgrade() {
                app.quit_now();
            }
        });
    }
    let _ = app.run();
}
