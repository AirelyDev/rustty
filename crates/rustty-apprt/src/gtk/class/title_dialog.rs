//! rustty-apprt::gtk::class::title_dialog
//!
//! PORTS: ghostty `src/apprt/gtk/class/title_dialog.zig` (`GhosttyTitleDialog`).
//!
//! An `adw::AlertDialog` (NOT our `Dialog` base — the `.blp` is
//! `template $GhosttyTitleDialog: Adw.AlertDialog`, `title-dialog.blp:4`) with a
//! single `Entry` child. It prompts the user for a new terminal/tab title and
//! emits `set` with the entered string when the user confirms (`title_dialog.zig`).

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

use crate::gtk::ext;

/// The target of a title dialog.
///
/// PORTS: `title_dialog.zig:220-234` `Target`.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "GhosttyTitleDialogTarget")]
#[repr(i32)]
pub enum Target {
    /// PORTS: `title_dialog.zig:221` `.surface`.
    #[default]
    Surface,
    /// PORTS: `title_dialog.zig:222` `.tab`.
    Tab,
}

impl Target {
    /// PORTS: `title_dialog.zig:223-228` `title`. These strings carry no product
    /// name, so no R2 rebrand applies. (See `close_confirmation_dialog.rs`
    /// `Target::title` for the i18n PORT NOTE — the reference wraps each in
    /// `i18n._(...)`; rustty emits the untranslated literal.)
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Target::Surface => "Change Terminal Title",
            Target::Tab => "Change Tab Title",
        }
    }
}

mod imp {
    use super::*;

    /// PORTS: `title_dialog.zig:16-218` `TitleDialog` + its `Private` (`:77-86`).
    /// A `CompositeTemplate` over `ui/1.5/title-dialog.ui`. Unlike
    /// `close_confirmation_dialog.rs` (which has no template children and so loads
    /// the template purely in `class_init` via `set_template_from_resource`), this
    /// dialog binds the `entry` child (`title_dialog.zig:83,201`), so it uses the
    /// `#[template(resource = ...)]` attribute + `#[template_child]` + `init_template`
    /// idiom (matching `tab.rs`/`surface.rs`).
    #[derive(Default, Properties, gtk4::CompositeTemplate)]
    #[properties(wrapper_type = super::TitleDialog)]
    #[template(resource = "/io/airely/rustty/ui/1.5/title-dialog.ui")]
    pub struct TitleDialog {
        /// PORTS: `title_dialog.zig:29-46,82` the `target` property/private field.
        #[property(get, set, builder(Target::default()))]
        pub target: Cell<Target>,

        /// PORTS: `title_dialog.zig:47-60,78-79` the `initial-value` property /
        /// private field — the text pre-filled into the entry.
        #[property(get, set, name = "initial-value", nullable)]
        pub initial_value: RefCell<Option<String>>,

        /// PORTS: `title_dialog.zig:83` the `entry` template child.
        #[template_child]
        pub entry: TemplateChild<gtk4::Entry>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TitleDialog {
        // PORTS: `title_dialog.zig:21` `.name`.
        const NAME: &'static str = "GhosttyTitleDialog";
        type Type = super::TitleDialog;
        // PORTS: `title_dialog.zig:19` `pub const Parent = adw.AlertDialog`.
        type ParentType = libadwaita::AlertDialog;

        /// PORTS: `title_dialog.zig:187-212` `Class.init` — load the template,
        /// bind children, register the enum GType.
        fn class_init(klass: &mut Self::Class) {
            Target::ensure_type();
            klass.bind_template();
        }

        /// PORTS: `title_dialog.zig:88-90` `init` — `gtk.Widget.initTemplate`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for TitleDialog {
        /// PORTS: `title_dialog.zig:63-75` `signals` — `set` with one string param.
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![glib::subclass::Signal::builder("set")
                    .param_types([String::static_type()])
                    .build()]
            })
        }
    }

    impl WidgetImpl for TitleDialog {}
    impl AdwDialogImpl for TitleDialog {}
    impl AdwAlertDialogImpl for TitleDialog {}
}

glib::wrapper! {
    /// PORTS: `title_dialog.zig:16` `pub const TitleDialog`.
    pub struct TitleDialog(ObjectSubclass<imp::TitleDialog>)
        @extends libadwaita::AlertDialog, libadwaita::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl TitleDialog {
    /// PORTS: `title_dialog.zig:92-94` `new`.
    #[must_use]
    pub fn new(target: Target, initial_value: Option<&str>) -> Self {
        glib::Object::builder()
            .property("target", target)
            .property("initial-value", initial_value)
            .build()
    }

    /// PORTS: `title_dialog.zig:96-128` `present`. Prefer a window ancestor to
    /// attach to, pre-fill the entry, set the heading, then present async.
    pub fn present(&self, parent: &gtk4::Widget) {
        // If we have a window we can attach to, we prefer that.
        // PORTS: `title_dialog.zig:98-109`.
        let chosen: gtk4::Widget =
            if let Some(window) = ext::ancestor::<libadwaita::ApplicationWindow>(parent) {
                window.upcast()
            } else if let Some(window) = ext::ancestor::<libadwaita::Window>(parent) {
                window.upcast()
            } else {
                parent.clone()
            };

        // Set our initial value. PORTS: `title_dialog.zig:111-115`.
        if let Some(v) = self.initial_value() {
            self.imp().entry.buffer().set_text(&v);
        }

        // Set the title for the dialog. PORTS: `title_dialog.zig:117-118`.
        self.set_heading(Some(self.target().title()));

        // Show it. PORTS: `title_dialog.zig:120-127` (`choose` + `alertDialogReady`).
        // The gtk4-rs `choose` callback receives the response id directly (no
        // `choose_finish`); on "ok" we emit `set` with the entry text.
        self.clone().choose(
            Some(&chosen),
            gtk4::gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |response| {
                    // PORTS: `title_dialog.zig:138-139` — only act on "ok".
                    if response == "ok" {
                        // PORTS: `title_dialog.zig:141-148`.
                        let title = this.imp().entry.buffer().text();
                        this.emit_by_name::<()>("set", &[&title.as_str()]);
                    }
                }
            ),
        );
    }
}
