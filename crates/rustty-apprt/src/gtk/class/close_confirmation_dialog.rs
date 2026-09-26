//! rustty-apprt::gtk::class::close_confirmation_dialog
//!
//! PORTS: ghostty `src/apprt/gtk/class/close_confirmation_dialog.zig`
//! (`GhosttyCloseConfirmationDialog`).
//!
//! A [`Dialog`] (our subclassable `adw::AlertDialog` base) that asks the user to
//! confirm closing the app / a tab / a window / a split. It emits `close-request`
//! when the user confirms and `cancel` otherwise; the call sites wire those to
//! the actual teardown (`window.zig:1400-1496`, `split_tree.zig:696-738`).

use std::cell::Cell;
use std::sync::OnceLock;

use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

use crate::gtk::class::dialog::{Dialog, DialogImpl};
use crate::gtk::gresource;
use crate::gtk::i18n::tr;

/// The target of a close dialog.
///
/// This is here so that we can consolidate all logic related to
/// prompting the user and closing windows/tabs/surfaces/etc.
/// together into one struct that is the sole source of truth.
///
/// PORTS: `close_confirmation_dialog.zig:171-204` `Target`.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "GhosttyCloseConfirmationDialogTarget")]
#[repr(i32)]
pub enum Target {
    /// PORTS: `close_confirmation_dialog.zig:177` `.app`.
    #[default]
    App,
    /// PORTS: `close_confirmation_dialog.zig:178` `.tab`.
    Tab,
    /// PORTS: `close_confirmation_dialog.zig:179` `.window`.
    Window,
    /// PORTS: `close_confirmation_dialog.zig:180` `.surface`.
    Surface,
}

impl Target {
    /// PORTS: `close_confirmation_dialog.zig:182-189` `title`.
    ///
    /// PORT NOTE (R2 identity): the reference's `.app` title is "Quit Ghostty?";
    /// rustty rebrands it to "Quit Rustty?" — and so do all 31 catalogs, whose
    /// msgid is `Quit Rustty?` (`po/io.airely.rustty.pot`), so the rebranded
    /// spelling is also the lookup KEY. The other three are verbatim.
    ///
    /// PORTS the reference's `i18n._(...)` wrapping (`:182-189`) via
    /// [`crate::gtk::i18n::tr`]. This is a function, not a `const` table, for the
    /// same reason the reference's is: gettext cannot run at comptime.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Target::App => tr(c"Quit Rustty?"),
            Target::Tab => tr(c"Close Tab?"),
            Target::Window => tr(c"Close Window?"),
            Target::Surface => tr(c"Close Split?"),
        }
    }

    /// PORTS: `close_confirmation_dialog.zig:191-198` `body`, including its
    /// `i18n._(...)` wrapping. (See `title` for the R2/msgid note.)
    #[must_use]
    pub fn body(self) -> &'static str {
        match self {
            Target::App => tr(c"All terminal sessions will be terminated."),
            Target::Tab => tr(c"All terminal sessions in this tab will be terminated."),
            Target::Window => tr(c"All terminal sessions in this window will be terminated."),
            Target::Surface => {
                tr(c"The currently running process in this split will be terminated.")
            }
        }
    }
}

mod imp {
    use super::*;

    /// PORTS: `close_confirmation_dialog.zig:12-169` `CloseConfirmationDialog` +
    /// its `Private` (`:68-71`). A `CompositeTemplate` over
    /// `ui/1.2/close-confirmation-dialog.ui` (the `responses`/`close-response`
    /// live in the `.blp`, so there are no template children or callbacks here).
    #[derive(Default, Properties, gtk4::CompositeTemplate)]
    #[properties(wrapper_type = super::CloseConfirmationDialog)]
    #[template(resource = "/io/airely/rustty/ui/1.2/close-confirmation-dialog.ui")]
    pub struct CloseConfirmationDialog {
        /// PORTS: `close_confirmation_dialog.zig:24-42,68-70` the `target`
        /// property/private field. `Target` derives `glib::HasParamSpec` via
        /// `glib::Enum`, so it backs a `#[property]` directly.
        #[property(get, set, builder(Target::default()))]
        pub target: Cell<Target>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CloseConfirmationDialog {
        // PORTS: `close_confirmation_dialog.zig:18` `.name`.
        const NAME: &'static str = "GhosttyCloseConfirmationDialog";
        type Type = super::CloseConfirmationDialog;
        // PORTS: `close_confirmation_dialog.zig:15` `pub const Parent = Dialog`.
        type ParentType = Dialog;

        /// PORTS: `close_confirmation_dialog.zig:142-165` `Class.init`.
        fn class_init(klass: &mut Self::Class) {
            // `ensureType(Dialog)` (`:143`) so the template's `$GhosttyDialog`
            // parent resolves; register the enum GType too.
            Dialog::ensure_type();
            Target::ensure_type();

            // `setTemplateFromResource(gresource.blueprint(1, 2,
            // "close-confirmation-dialog"))` (`:144-151`).
            klass.set_template_from_resource(gresource::blueprint(
                1,
                2,
                "close-confirmation-dialog",
            ));
        }

        /// PORTS: `close_confirmation_dialog.zig:79-81` `init` —
        /// `gtk.Widget.initTemplate`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for CloseConfirmationDialog {
        /// PORTS: `close_confirmation_dialog.zig:44-66` `signals` — `close-request`
        /// and `cancel`, both no-param.
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // `close-request` (`:45-54`).
                    glib::subclass::Signal::builder("close-request").build(),
                    // `cancel` (`:56-65`).
                    glib::subclass::Signal::builder("cancel").build(),
                ]
            })
        }
    }

    impl WidgetImpl for CloseConfirmationDialog {}
    impl AdwDialogImpl for CloseConfirmationDialog {}

    impl AdwAlertDialogImpl for CloseConfirmationDialog {
        /// PORTS: `close_confirmation_dialog.zig:97-116` `response`. On the "close"
        /// response emit `close-request`; on anything else emit `cancel`.
        fn response(&self, response: &str) {
            let obj = self.obj();
            if response == "close" {
                obj.emit_by_name::<()>("close-request", &[]);
            } else {
                obj.emit_by_name::<()>("cancel", &[]);
            }
        }
    }

    // Marker: `CloseConfirmationDialog` is a valid subclass of our `Dialog` base.
    impl DialogImpl for CloseConfirmationDialog {}
}

glib::wrapper! {
    /// PORTS: `close_confirmation_dialog.zig:12` `pub const CloseConfirmationDialog`.
    pub struct CloseConfirmationDialog(ObjectSubclass<imp::CloseConfirmationDialog>)
        @extends Dialog, libadwaita::AlertDialog, libadwaita::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl CloseConfirmationDialog {
    /// PORTS: `close_confirmation_dialog.zig:73-77` `new`.
    #[must_use]
    pub fn new(target: Target) -> Self {
        glib::Object::builder().property("target", target).build()
    }

    /// PORTS: `close_confirmation_dialog.zig:83-91` `present`. Set the heading/body
    /// from the target, then present over `parent`.
    pub fn present(&self, parent: Option<&gtk4::Widget>) {
        // Setup our title/body text.
        let target = self.target();
        self.set_heading(Some(target.title()));
        self.set_body(target.body());

        // Show it
        AdwDialogExt::present(self, parent);
    }
}
