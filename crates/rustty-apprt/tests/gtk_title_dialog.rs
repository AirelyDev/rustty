//! T23 TITLE-DIALOG KAT — exercises TitleDialog construction at runtime.
//!
//! Compile-green does not prove the CompositeTemplate loads or the `entry` template
//! child binds. This drives construction (which loads ui/1.5/title-dialog.ui and binds
//! `entry`), the `target`/`initial-value` properties, and confirms the template's Entry
//! is present in the widget tree. (The async `choose` present + "set" emission needs a
//! real dialog response; that path is verified by running the app.)
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")`.
#![cfg(all(target_os = "linux", feature = "gtk"))]
#![allow(non_snake_case)]

use std::time::Duration;

use allocator_api2::alloc::Global;
use gio::prelude::*;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::title_dialog::{Target, TitleDialog};
use rustty_apprt::gtk::{self, register_resources};
use rustty_core::app::App as CoreApp;

#[test]
#[ignore = "headed GTK; run: cargo test -p rustty-apprt --features gtk \
            --test gtk_title_dialog -- --ignored --exact title_dialog_constructs_and_binds_entry"]
fn title_dialog_constructs_and_binds_entry() {
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

    // Construction loads the CompositeTemplate + binds the `entry` child. If the .ui is
    // missing from the bundle or the child name is wrong, this aborts here.
    let dialog = TitleDialog::new(Target::Tab, Some("initial-xyz"));
    assert_eq!(dialog.target(), Target::Tab, "target round-trips");
    assert_eq!(
        dialog.initial_value().as_deref(),
        Some("initial-xyz"),
        "initial-value round-trips"
    );
    assert_eq!(Target::Tab.title(), "Change Tab Title");
    assert_eq!(Target::Surface.title(), "Change Terminal Title");

    // The template CONTENT loaded: the .blp sets the body text on the AlertDialog.
    assert!(
        dialog.body().contains("blank"),
        "the title-dialog template body loaded (got {:?})",
        dialog.body()
    );

    // Exercise present(): it accesses self.imp().entry.buffer() (the bound template
    // child) to seed the initial value, then set_heading, then choose(). If the `entry`
    // child failed to bind, this panics on the unbound TemplateChild. A real parent
    // window is provided so choose() has a toplevel to attach to.
    let window = libadwaita::ApplicationWindow::new(&application);
    window.present();
    let ctx = glib::MainContext::default();
    for _ in 0..10 {
        ctx.iteration(false);
    }
    dialog.present(window.upcast_ref::<gtk4::Widget>());
    for _ in 0..5 {
        ctx.iteration(false);
    }
    assert_eq!(
        dialog.heading().as_deref(),
        Some("Change Tab Title"),
        "present() set the heading from the target"
    );
    // Dismiss the async dialog before teardown.
    dialog.force_close();
    for _ in 0..5 {
        ctx.iteration(false);
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
