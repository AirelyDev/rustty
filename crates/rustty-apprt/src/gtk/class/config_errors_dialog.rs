//! rustty-apprt::gtk::class::config_errors_dialog
//!
//! PORTS: ghostty `src/apprt/gtk/class/config_errors_dialog.zig`
//! (`GhosttyConfigErrorsDialog`).
//!
//! A [`Dialog`] (our subclassable `adw::AlertDialog` base) that shows the
//! configuration diagnostics and offers "Ignore" / "Reload Configuration". The
//! `reload` response emits the `reload-config` signal, which
//! `Application::show_config_errors_dialog` wires to an actual config reload.
//!
//! The `.blp` (`ui/1.2/config-errors-dialog.blp`) binds the `TextView`'s buffer
//! to `(template.config as <$GhosttyConfig>).diagnostics-buffer`, so the whole
//! body of the dialog is derived from the `config` property below.

use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

use crate::gtk::class::config::Config;
use crate::gtk::class::dialog::{Dialog, DialogImpl};
use crate::gtk::gresource;

mod imp {
    use super::*;

    /// PORTS: `config_errors_dialog.zig:12-152` `ConfigErrorsDialog` + its
    /// `Private` (`:57-60`). A `CompositeTemplate` over
    /// `ui/1.2/config-errors-dialog.ui`; the heading/body/responses and the
    /// bound `TextView` all live in the `.blp`, so there are no template
    /// children or callbacks here.
    #[derive(Default, Properties, gtk4::CompositeTemplate)]
    #[properties(wrapper_type = super::ConfigErrorsDialog)]
    #[template(resource = "/io/airely/rustty/ui/1.2/config-errors-dialog.ui")]
    pub struct ConfigErrorsDialog {
        /// PORTS: `config_errors_dialog.zig:24-39,57-58` the `config` property /
        /// private field (`?*Config`, `privateObjFieldAccessor`-shaped: getter
        /// transfer `.none`, setter transfer `.full`).
        ///
        /// PORT NOTE: the reference hand-rolls `getConfig`/`setConfig`
        /// (`:132-145`) — the setter unrefs the prior value and does NOT ref the
        /// new one because "the property setter handles it (uses GValue.get vs.
        /// take)". `glib::Properties` on a `RefCell<Option<Config>>` performs
        /// exactly the same ownership dance: the old value is dropped (unref'd)
        /// and the new strong ref is stored. The reference's `dispose`
        /// (`:120-124`) unref + null is likewise the `RefCell` drop in
        /// [`ObjectImpl::dispose`] below.
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ConfigErrorsDialog {
        // PORTS: `config_errors_dialog.zig:18` `.name`.
        const NAME: &'static str = "GhosttyConfigErrorsDialog";
        type Type = super::ConfigErrorsDialog;
        // PORTS: `config_errors_dialog.zig:15` `pub const Parent = Dialog`.
        type ParentType = Dialog;

        /// PORTS: `config_errors_dialog.zig:126-152` `Class.init`.
        fn class_init(klass: &mut Self::Class) {
            // `gobject.ext.ensureType(Dialog)` (`:127`) so the template's
            // `$GhosttyDialog` parent resolves. `Config` is ensured too because
            // the template's `bind (template.config as <$GhosttyConfig>)`
            // expression names the `GhosttyConfig` GType by name.
            Dialog::ensure_type();
            Config::ensure_type();

            // `setTemplateFromResource(gresource.blueprint(.{ .major = 1,
            // .minor = 2, .name = "config-errors-dialog" }))` (`:128-135`).
            klass.set_template_from_resource(gresource::blueprint(1, 2, "config-errors-dialog"));
        }

        /// PORTS: `config_errors_dialog.zig:68-70` `init` —
        /// `gtk.Widget.initTemplate`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ConfigErrorsDialog {
        /// PORTS: `config_errors_dialog.zig:41-55` `signals` — `reload-config`,
        /// no params, `void` return.
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("reload-config").build()])
        }

        /// PORTS: `config_errors_dialog.zig:118-130` `dispose`. The reference
        /// unrefs + nulls `priv.config` (`:120-124`), calls
        /// `gtk.Widget.disposeTemplate` (`:126-129`) and then chains the parent
        /// dispose (`:131-134`) — the gtk4-rs dispose glue chains up for us.
        fn dispose(&self) {
            // `if (priv.config) |v| { v.unref(); priv.config = null; }`
            // (`:120-124`) — dropping the `Option<Config>` unrefs it.
            self.config.replace(None);

            // `gtk.Widget.disposeTemplate(self.as(gtk.Widget), getGObjectType())`
            // (`:126-129`).
            self.obj().dispose_template();
        }
    }

    impl WidgetImpl for ConfigErrorsDialog {}
    impl AdwDialogImpl for ConfigErrorsDialog {}

    impl AdwAlertDialogImpl for ConfigErrorsDialog {
        /// PORTS: `config_errors_dialog.zig:82-93` `response`. Only the `reload`
        /// response is interesting; everything else (`ignore`, or a dismissal)
        /// is ignored.
        fn response(&self, response: &str) {
            // `if (std.mem.orderZ(u8, response_id, "reload") != .eq) return;`
            // (`:86`).
            if response != "reload" {
                return;
            }

            // `signals.@"reload-config".impl.emit(self, null, .{}, null)`
            // (`:87-92`).
            self.obj().emit_by_name::<()>("reload-config", &[]);
        }
    }

    // Marker: `ConfigErrorsDialog` is a valid subclass of our `Dialog` base.
    impl DialogImpl for ConfigErrorsDialog {}
}

glib::wrapper! {
    /// PORTS: `config_errors_dialog.zig:12` `pub const ConfigErrorsDialog`.
    pub struct ConfigErrorsDialog(ObjectSubclass<imp::ConfigErrorsDialog>)
        @extends Dialog, libadwaita::AlertDialog, libadwaita::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl ConfigErrorsDialog {
    /// PORTS: `config_errors_dialog.zig:62-66` `new`.
    #[must_use]
    pub fn new(config: &Config) -> Self {
        glib::Object::builder().property("config", config).build()
    }

    /// PORTS: `config_errors_dialog.zig:72-74` `present` —
    /// `self.as(Dialog).present(parent)`.
    pub fn present(&self, parent: Option<&gtk4::Widget>) {
        AdwDialogExt::present(self, parent);
    }

    /// PORTS: `config_errors_dialog.zig:76-78` `close` —
    /// `self.as(Dialog).close()`, which under the AD-3 collapse is
    /// `adw.Dialog.forceClose()` (`dialog.zig:64-69`).
    pub fn close(&self) {
        AdwDialogExt::force_close(self);
    }
}
