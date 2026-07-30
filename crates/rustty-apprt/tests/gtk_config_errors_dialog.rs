//! WS20 CONFIG-ERRORS-DIALOG KAT — exercises the ConfigErrorsDialog at runtime.
//!
//! Compile-time green does NOT prove this works: the risky bits are all runtime —
//! (1) the CompositeTemplate loads from the gresource (`$GhosttyConfigErrorsDialog:
//! $GhosttyDialog` needs the GhosttyDialog GType registered AND the .ui in the bundle;
//! the `.blp` has shipped since T-blueprints but nothing had ever LOADED it),
//! (2) the template's `bind (template.config as <$GhosttyConfig>).diagnostics-buffer`
//! expression resolves — which needs the `config` property registered with exactly the
//! `GhosttyConfig` value type and the `GhosttyConfig` GType ensured at class_init,
//! (3) the `response` vfunc override dispatches through the custom `Dialog` base and
//! maps ONLY "reload" to the `reload-config` signal (`config_errors_dialog.zig:82-93`).
//!
//! G-GTKINIT: exactly ONE headed `#[test]` lives in this binary.
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")`.
#![cfg(all(target_os = "linux", feature = "gtk"))]
#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use allocator_api2::alloc::Global;
use gio::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::config::Config;
use rustty_apprt::gtk::class::config_errors_dialog::ConfigErrorsDialog;
use rustty_apprt::gtk::{self, register_resources};
use rustty_core::app::App as CoreApp;

#[test]
#[ignore = "headed GTK; run: cargo test -p rustty-apprt --features gtk \
            --test gtk_config_errors_dialog -- --ignored --exact config_errors_dialog_reload_response"]
fn config_errors_dialog_reload_response() {
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

    // (1)+(2) Construction loads the CompositeTemplate from the gresource AND
    //         evaluates the `template.config as <$GhosttyConfig>` binding. If the
    //         .ui is missing, GhosttyDialog is unregistered, or the `config`
    //         property's value type is wrong, this aborts here.
    let config: Config = glib::Object::builder().build();
    let dialog = ConfigErrorsDialog::new(&config);
    assert_eq!(
        dialog.property::<Option<Config>>("config").as_ref(),
        Some(&config),
        "the config property round-trips (`config_errors_dialog.zig:132-145`)"
    );

    let reload_fired = Rc::new(Cell::new(0u32));
    {
        let c = reload_fired.clone();
        dialog.connect_local("reload-config", false, move |_| {
            c.set(c.get() + 1);
            None
        });
    }

    // (3) `if (std.mem.orderZ(u8, response_id, "reload") != .eq) return;`
    //     (`config_errors_dialog.zig:86`) — only "reload" emits.
    dialog.emit_by_name::<()>("response", &[&"ignore"]);
    assert_eq!(
        reload_fired.get(),
        0,
        "response \"ignore\" must NOT emit reload-config"
    );

    dialog.emit_by_name::<()>("response", &[&"reload"]);
    assert_eq!(
        reload_fired.get(),
        1,
        "response \"reload\" emits reload-config"
    );

    // present/close must not abort (`config_errors_dialog.zig:72-78`).
    dialog.present(None);
    let ctx = glib::MainContext::default();
    for _ in 0..10 {
        ctx.iteration(false);
    }
    dialog.close();

    // Setting a fresh config must unref the prior one without aborting
    // (`config_errors_dialog.zig:138-145`), and clearing it must be legal
    // (the property is nullable).
    let config2: Config = glib::Object::builder().build();
    dialog.set_property("config", &config2);
    assert_eq!(
        dialog.property::<Option<Config>>("config").as_ref(),
        Some(&config2)
    );
    dialog.set_property("config", None::<Config>);
    assert!(dialog.property::<Option<Config>>("config").is_none());

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
