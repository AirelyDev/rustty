//! rustty-apprt::gtk::class::clipboard_confirmation_dialog
//!
//! PORTS: ghostty `src/apprt/gtk/class/clipboard_confirmation_dialog.zig`
//! (`GhosttyClipboardConfirmationDialog`).
//!
//! A [`Dialog`] (our subclassable `adw::AlertDialog` base) that asks the user to
//! authorize a clipboard access (OSC 52 read/write) or confirm a potentially
//! unsafe paste. It emits `confirm` (with the "remember" flag) when the user
//! allows the access and `deny` (also with "remember") otherwise; the call sites
//! in `surface.rs` (`show_clipboard_confirmation` / `clipboard_confirmation_*`)
//! wire those to `completeClipboardRequest` and the config-persistence
//! (`surface.zig:3945-4041`).
//!
//! **AD-3 COLLAPSE:** the reference gates the "remember" switch behind
//! `adw_version.supportsSwitchRow()` (`clipboard_confirmation_dialog.zig:16-17`),
//! making `can_remember`, the `remember_choice` template child, and the
//! `response`-time read all comptime-conditional. rustty targets adw ≥ 1.5
//! UNCONDITIONALLY (see `dialog.rs`), so `supportsSwitchRow()` is always true: we
//! drop the comptime gate entirely — `remember_choice` is always bound and always
//! read in `response`. The `can-remember` property survives (the `.blp` binds
//! `visible: bind template.can-remember`), toggling only the row's *visibility*.

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

use rustty_apprt_api::structs::ClipboardRequest;

use crate::gtk::class::dialog::{Dialog, DialogImpl};
use crate::gtk::gresource;
use crate::gtk::i18n::tr;

/// A `#[glib::Boxed]` newtype so an [`ClipboardRequest`] can back the `request`
/// GObject property.
///
/// PORTS: the reference's `?*apprt.ClipboardRequest` boxed field
/// (`clipboard_confirmation_dialog.zig:50-60,119`) — a heap `apprt.ClipboardRequest`
/// owned by the dialog and exposed through `C.privateBoxedFieldAccessor`. Here the
/// value is `Copy`, so the boxed type carries it by value (no heap indirection);
/// the derive/attribute mirror `split_tree.rs`'s `Tree`
/// (`datastruct/split_tree.zig:1315`).
#[derive(Clone, Copy, glib::Boxed)]
#[boxed_type(name = "GhosttyClipboardRequest", nullable)]
pub struct ClipboardRequestBoxed(pub ClipboardRequest);

mod imp {
    use super::*;

    /// PORTS: `clipboard_confirmation_dialog.zig:19-352` `ClipboardConfirmationDialog` +
    /// its `Private` (`:117-138`). A `CompositeTemplate` over
    /// `ui/1.4/clipboard-confirmation-dialog.ui` that binds the four content
    /// children + the `remember_choice` switch and the reveal/hide/notify template
    /// callbacks.
    #[derive(Default, Properties, gtk4::CompositeTemplate)]
    #[properties(wrapper_type = super::ClipboardConfirmationDialog)]
    #[template(resource = "/io/airely/rustty/ui/1.4/clipboard-confirmation-dialog.ui")]
    pub struct ClipboardConfirmationDialog {
        /// The request that this dialog is for.
        ///
        /// PORTS: `clipboard_confirmation_dialog.zig:50-60,119` the `request`
        /// property / private field. Nullable boxed (transfer: get `.none`, set
        /// `.full`), matching `C.privateBoxedFieldAccessor("request")`.
        #[property(get, set, name = "request", nullable)]
        pub request: RefCell<Option<super::ClipboardRequestBoxed>>,

        /// The clipboard contents being read/written.
        ///
        /// PORTS: `clipboard_confirmation_dialog.zig:62-72,122` the
        /// `clipboard-contents` property / private field — the `TextBuffer` shown
        /// in the dialog. Nullable GObject (transfer: get `.none`, set `.full`).
        #[property(get, set, name = "clipboard-contents", nullable)]
        pub clipboard_contents: RefCell<Option<gtk4::TextBuffer>>,

        /// Whether the contents should be blurred.
        ///
        /// PORTS: `clipboard_confirmation_dialog.zig:74-90,125` the `blur` property /
        /// private field — whether the contents are blurred behind a reveal button.
        #[property(get, set, name = "blur")]
        pub blur: Cell<bool>,

        /// Whether the user can remember the choice.
        ///
        /// PORTS: `clipboard_confirmation_dialog.zig:32-48,128` the `can-remember`
        /// property / private field. AD-3 COLLAPSE: no longer gates the switch's
        /// existence (always bound); only drives `remember_choice`'s `visible` bind.
        #[property(get, set, name = "can-remember")]
        pub can_remember: Cell<bool>,

        /// PORTS: `clipboard_confirmation_dialog.zig:131` `text_view_scroll`.
        #[template_child]
        pub text_view_scroll: TemplateChild<gtk4::ScrolledWindow>,

        /// PORTS: `clipboard_confirmation_dialog.zig:132` `text_view`.
        #[template_child]
        pub text_view: TemplateChild<gtk4::TextView>,

        /// PORTS: `clipboard_confirmation_dialog.zig:133` `reveal_button`.
        #[template_child]
        pub reveal_button: TemplateChild<gtk4::Button>,

        /// PORTS: `clipboard_confirmation_dialog.zig:134` `hide_button`.
        #[template_child]
        pub hide_button: TemplateChild<gtk4::Button>,

        /// PORTS: `clipboard_confirmation_dialog.zig:135` `remember_choice`.
        /// AD-3 COLLAPSE: unconditional (`*adw.SwitchRow`, never `void`).
        #[template_child]
        pub remember_choice: TemplateChild<libadwaita::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ClipboardConfirmationDialog {
        // PORTS: `clipboard_confirmation_dialog.zig:24` `.name`.
        const NAME: &'static str = "GhosttyClipboardConfirmationDialog";
        type Type = super::ClipboardConfirmationDialog;
        // PORTS: `clipboard_confirmation_dialog.zig:22` `pub const Parent = Dialog`.
        type ParentType = Dialog;

        /// PORTS: `clipboard_confirmation_dialog.zig:298-346` `Class.init`.
        fn class_init(klass: &mut Self::Class) {
            // `ensureType(Dialog)` so the template's `$GhosttyDialog` parent
            // resolves; register the boxed request GType too (the `request`
            // property + `notify::request` bind need it interned).
            Dialog::ensure_type();
            super::ClipboardRequestBoxed::ensure_type();

            // `setTemplateFromResource(gresource.blueprint(1, 4,
            // "clipboard-confirmation-dialog"))` (`:299-313`).
            klass.set_template_from_resource(gresource::blueprint(
                1,
                4,
                "clipboard-confirmation-dialog",
            ));

            // Bind the template children + the reveal/hide/notify callbacks
            // (`:315-328`).
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        /// PORTS: `clipboard_confirmation_dialog.zig:144-150` `init` —
        /// `gtk.Widget.initTemplate` (the initial `propBlur`/`propRequest` trigger
        /// is deferred to `constructed`, once the wrapper exists to call them on).
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ClipboardConfirmationDialog {
        /// PORTS: `clipboard_confirmation_dialog.zig:93-115` `signals` — `confirm`
        /// and `deny`, both with a single `bool` "remember" param.
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // `confirm` (`:105-114`).
                    glib::subclass::Signal::builder("confirm")
                        .param_types([bool::static_type()])
                        .build(),
                    // `deny` (`:94-103`).
                    glib::subclass::Signal::builder("deny")
                        .param_types([bool::static_type()])
                        .build(),
                ]
            })
        }

        /// PORTS: `clipboard_confirmation_dialog.zig:144-150` `init` — after the
        /// template is initialized, trigger the initial `blur`/`request` values so
        /// the heading/body and blur state reflect the default (the reference calls
        /// `propBlur`/`propRequest` with a null pspec here).
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.apply_blur();
            obj.apply_request();
        }
    }

    impl WidgetImpl for ClipboardConfirmationDialog {}
    impl AdwDialogImpl for ClipboardConfirmationDialog {}

    impl AdwAlertDialogImpl for ClipboardConfirmationDialog {
        /// PORTS: `clipboard_confirmation_dialog.zig:230-254` `response`. Read the
        /// "remember" switch, then emit `deny` on "cancel" / `confirm` on "ok".
        ///
        /// AD-3 COLLAPSE: the reference's `if (comptime can_remember)` gate is gone
        /// — `remember_choice` always exists, so we always read `is_active()`.
        fn response(&self, response: &str) {
            let remember = self.remember_choice.is_active();
            let obj = self.obj();
            if response == "cancel" {
                obj.emit_by_name::<()>("deny", &[&remember]);
            } else if response == "ok" {
                obj.emit_by_name::<()>("confirm", &[&remember]);
            }
        }
    }

    // Marker: `ClipboardConfirmationDialog` is a valid subclass of our `Dialog` base.
    impl DialogImpl for ClipboardConfirmationDialog {}
}

glib::wrapper! {
    /// PORTS: `clipboard_confirmation_dialog.zig:19` `pub const ClipboardConfirmationDialog`.
    pub struct ClipboardConfirmationDialog(ObjectSubclass<imp::ClipboardConfirmationDialog>)
        @extends Dialog, libadwaita::AlertDialog, libadwaita::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl ClipboardConfirmationDialog {
    /// PORTS: `clipboard_confirmation_dialog.zig:140-142` `new`.
    #[must_use]
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// PORTS: `clipboard_confirmation_dialog.zig:152-154` `present`.
    pub fn present(&self, parent: Option<&gtk4::Widget>) {
        AdwDialogExt::present(self, parent);
    }

    /// Get the clipboard request without copying.
    ///
    /// PORTS: `clipboard_confirmation_dialog.zig:156-159` `getRequest` (a cheap
    /// `Copy` here).
    #[must_use]
    pub fn get_request(&self) -> Option<ClipboardRequestBoxed> {
        *self.imp().request.borrow()
    }

    /// Get the clipboard contents without copying.
    ///
    /// PORTS: `clipboard_confirmation_dialog.zig:161-164` `getClipboardContents`.
    #[must_use]
    pub fn get_clipboard_contents(&self) -> Option<gtk4::TextBuffer> {
        self.imp().clipboard_contents.borrow().clone()
    }

    /// PORTS: `clipboard_confirmation_dialog.zig:169-186` `propBlur`. Apply the
    /// current `blur` state to the content view + reveal/hide buttons.
    fn apply_blur(&self) {
        let imp = self.imp();
        let scroll = imp.text_view_scroll.upcast_ref::<gtk4::Widget>();
        let view = imp.text_view.upcast_ref::<gtk4::Widget>();
        let reveal = imp.reveal_button.upcast_ref::<gtk4::Widget>();
        let hide = imp.hide_button.upcast_ref::<gtk4::Widget>();
        if imp.blur.get() {
            scroll.set_sensitive(false);
            view.add_css_class("blurred");
            reveal.set_visible(true);
            hide.set_visible(false);
        } else {
            scroll.set_sensitive(true);
            view.remove_css_class("blurred");
            reveal.set_visible(false);
            hide.set_visible(false);
        }
    }

    /// PORTS: `clipboard_confirmation_dialog.zig:188-209` `propRequest`. Set the
    /// AlertDialog heading/body from the current `request` variant.
    ///
    /// PORTS the reference's `i18n._(...)` wrapping of all six strings
    /// (`clipboard_confirmation_dialog.zig:197,198,201,202,205,206`) via
    /// [`crate::gtk::i18n::tr`]. The msgids are byte-exact against the committed
    /// catalogs (`po/io.airely.rustty.pot`); none carries a product name, so no R2
    /// rebrand applies.
    fn apply_request(&self) {
        let Some(boxed) = self.get_request() else {
            return;
        };
        let (heading, body) = match boxed.0 {
            // .osc_52_write, .kitty_write => { ... },
            ClipboardRequest::Osc52Write(_) | ClipboardRequest::KittyWrite(_) => (
                tr(c"Authorize Clipboard Access"),
                tr(c"An application is attempting to write to the clipboard. The current clipboard contents are shown below."),
            ),
            // .osc_52_read, .kitty_read => { ... },
            ClipboardRequest::Osc52Read(_) | ClipboardRequest::KittyRead(_) => (
                tr(c"Authorize Clipboard Access"),
                tr(c"An application is attempting to read from the clipboard. The current clipboard contents are shown below."),
            ),
            ClipboardRequest::Paste(_) => (
                tr(c"Warning: Potentially Unsafe Paste"),
                tr(c"Pasting this text into the terminal may be dangerous as it looks like some commands may be executed."),
            ),
            // .list => unreachable,
            ClipboardRequest::List(_) => unreachable!(),
        };
        self.set_heading(Some(heading));
        self.set_body(body);
    }
}

impl Default for ClipboardConfirmationDialog {
    fn default() -> Self {
        Self::new()
    }
}

// Template callbacks bound in `class_init` via `bind_template_instance_callbacks`.
//
// SIGNATURE GOTCHA: these are NON-swapped (the `.blp` callbacks carry no
// `swapped`), so gtk4-rs delivers `[emitter, ...signal_args, this]` — the
// emitting widget FIRST and the template instance (`this`) LAST, matching the
// reference's Zig callback signatures (`revealButtonClicked(_: *gtk.Button,
// self: *Self)`, `propBlur(self, _: *ParamSpec, _: ?*anyopaque)`). Writing these
// as `&self` compiles but ABORTS at runtime with a WrongValueType.
#[gtk4::template_callbacks]
impl ClipboardConfirmationDialog {
    /// PORTS: `clipboard_confirmation_dialog.zig:327` `notify_blur` → `propBlur`.
    /// The `notify::blur` handler shape is `(emitter, pspec, this)` (see
    /// `window.rs` `on_notify_config`).
    // Renamed `on_notify_*` (blueprint name kept via `name = …`) so they don't
    // clash with the `glib::Properties`-generated `notify_<prop>()` emitters (the
    // same collision handled in `surface.rs` `on_notify_config`).
    #[template_callback(name = "notify_blur")]
    fn on_notify_blur(
        _obj: &ClipboardConfirmationDialog,
        _pspec: &glib::ParamSpec,
        this: &ClipboardConfirmationDialog,
    ) {
        this.apply_blur();
    }

    /// PORTS: `clipboard_confirmation_dialog.zig:328` `notify_request` →
    /// `propRequest`.
    #[template_callback(name = "notify_request")]
    fn on_notify_request(
        _obj: &ClipboardConfirmationDialog,
        _pspec: &glib::ParamSpec,
        this: &ClipboardConfirmationDialog,
    ) {
        this.apply_request();
    }

    /// PORTS: `clipboard_confirmation_dialog.zig:211-217` `revealButtonClicked`.
    #[template_callback(name = "reveal_clicked")]
    fn reveal_clicked(_btn: &gtk4::Button, this: &ClipboardConfirmationDialog) {
        let imp = this.imp();
        imp.text_view_scroll
            .upcast_ref::<gtk4::Widget>()
            .set_sensitive(true);
        imp.text_view
            .upcast_ref::<gtk4::Widget>()
            .remove_css_class("blurred");
        imp.hide_button
            .upcast_ref::<gtk4::Widget>()
            .set_visible(true);
        imp.reveal_button
            .upcast_ref::<gtk4::Widget>()
            .set_visible(false);
    }

    /// PORTS: `clipboard_confirmation_dialog.zig:219-225` `hideButtonClicked`.
    #[template_callback(name = "hide_clicked")]
    fn hide_clicked(_btn: &gtk4::Button, this: &ClipboardConfirmationDialog) {
        let imp = this.imp();
        imp.text_view_scroll
            .upcast_ref::<gtk4::Widget>()
            .set_sensitive(false);
        imp.text_view
            .upcast_ref::<gtk4::Widget>()
            .add_css_class("blurred");
        imp.hide_button
            .upcast_ref::<gtk4::Widget>()
            .set_visible(false);
        imp.reveal_button
            .upcast_ref::<gtk4::Widget>()
            .set_visible(true);
    }
}
