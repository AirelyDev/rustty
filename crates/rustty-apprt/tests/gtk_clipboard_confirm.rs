//! CLIPBOARD-CONFIRMATION KAT — FIRES the dialog's template callbacks and response vfunc.
//!
//! The recurring bug on this port: a non-swapped template callback declared `&self`
//! compiles fine but ABORTS at runtime (WrongValueType) when the signal fires. Static
//! review has missed this before — only firing the signal catches it. So this test SETS
//! the `request`/`blur` properties (firing the `notify::request`/`notify::blur` template
//! callbacks) and EMITS `response` (firing the AlertDialog response vfunc → confirm/deny),
//! and confirms none abort and the behavior matches ghostty.
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use allocator_api2::alloc::Global;
use gio::prelude::*;
use libadwaita::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::clipboard_confirmation_dialog::{
    ClipboardConfirmationDialog, ClipboardRequestBoxed,
};
use rustty_apprt::gtk::{self, register_resources};
use rustty_apprt_api::structs::{Clipboard, ClipboardRequest};
use rustty_core::app::App as CoreApp;

#[test]
#[ignore = "headed GTK; run: cargo test -p rustty-apprt --features gtk \
            --test gtk_clipboard_confirm -- --ignored --exact clipboard_confirmation_fires_callbacks"]
fn clipboard_confirmation_fires_callbacks() {
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

    // Construction: loads the CompositeTemplate + binds the 5 template children.
    let dialog = ClipboardConfirmationDialog::new();

    // (A) FIRE notify::request — setting the property emits notify::request, invoking the
    //     `notify_request` template callback. A mis-shaped (non-swapped) signature aborts
    //     here. apply_request must set the heading per the request variant.
    dialog.set_request(Some(ClipboardRequestBoxed(ClipboardRequest::Paste(
        Clipboard::Standard,
    ))));
    assert_eq!(
        dialog.heading().as_deref(),
        Some("Warning: Potentially Unsafe Paste"),
        "notify_request must fire and set the unsafe-paste heading"
    );

    dialog.set_request(Some(ClipboardRequestBoxed(ClipboardRequest::Osc52Write(
        Clipboard::Standard,
    ))));
    assert_eq!(
        dialog.heading().as_deref(),
        Some("Authorize Clipboard Access"),
        "notify_request must update the heading for osc52-write"
    );

    // (B) FIRE notify::blur — setting the property invokes the `notify_blur` template
    //     callback. Just reaching the next line proves it didn't abort.
    dialog.set_blur(true);
    dialog.set_blur(false);

    // (C) FIRE the response vfunc → confirm/deny signals.
    let confirm_remember: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let deny_remember: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    {
        let c = confirm_remember.clone();
        dialog.connect_local("confirm", false, move |args| {
            c.set(Some(args[1].get::<bool>().expect("confirm arg is bool")));
            None
        });
        let d = deny_remember.clone();
        dialog.connect_local("deny", false, move |args| {
            d.set(Some(args[1].get::<bool>().expect("deny arg is bool")));
            None
        });
    }
    dialog.emit_by_name::<()>("response", &[&"ok"]);
    assert_eq!(
        confirm_remember.get(),
        Some(false),
        "response \"ok\" emits confirm(remember=false by default)"
    );
    assert_eq!(
        deny_remember.get(),
        None,
        "response \"ok\" does not emit deny"
    );

    dialog.emit_by_name::<()>("response", &[&"cancel"]);
    assert_eq!(
        deny_remember.get(),
        Some(false),
        "response \"cancel\" emits deny(remember=false)"
    );

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
