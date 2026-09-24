//! rustty-apprt::gtk::class::dialog
//!
//! PORTS: ghostty `src/apprt/gtk/class/dialog.zig` (`GhosttyDialog`).
//!
//! A tiny subclassable base over `adw::AlertDialog`. In the reference `Dialog` is
//! a comptime abstraction over `adw.AlertDialog` **or** `adw.MessageDialog`,
//! chosen by the linked Adwaita version (`dialog.zig:17-24`).
//!
//! **AD-3 COLLAPSE:** rustty targets adw ≥ 1.5 UNCONDITIONALLY, so
//! `adw_version.supportsDialogs()` is always true and the `MessageDialog` arm
//! (`dialog.zig:24,42-58,67`) is dead. We drop it entirely and make `Parent`
//! always `adw::AlertDialog`; the `present`/`close` `switch (Parent)`
//! (`dialog.zig:38-70`) collapse to the single `AlertDialog`/`Dialog` case, which
//! subclasses call directly (`adw::Dialog::present` / `force_close`). The
//! reference forwards `Parent.virtual_methods.response` so subclasses can
//! reference it (`dialog.zig:32-36`); in gtk4-rs that is the inherited
//! `AdwAlertDialogImpl::response` vfunc, which our subclassable wiring below
//! propagates to any `DialogImpl`.
//!
//! The dropped `adw.MessageDialog` arm of `present` (`dialog.zig:42-57`), quoted:
//!
//! ```text
//! adw.MessageDialog => {
//!     // Reset the previous parent window
//!     self.as(gtk.Window).setTransientFor(null);
//!
//!     // We need to get the window for the parent in order
//!     // to set the transient-for property on the MessageDialog.
//!     if (parent) |widget| parent: { ... self.as(gtk.Window).setTransientFor(window); }
//!
//!     self.as(gtk.Window).present();
//! },
//! ```

use glib::subclass::types::IsSubclassable;
use gtk4::subclass::prelude::*;
use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;

    /// PORTS: `dialog.zig:17` `pub const Dialog`. No private state — a pure base
    /// class whose only job is to exist as a subclassable `adw::AlertDialog`
    /// (`dialog.zig:21-24`, AD-3 collapse: the `MessageDialog` arm is dropped).
    #[derive(Default)]
    pub struct Dialog;

    #[glib::object_subclass]
    impl ObjectSubclass for Dialog {
        // PORTS: `dialog.zig:27` `.name = "GhosttyDialog"`.
        const NAME: &'static str = "GhosttyDialog";
        type Type = super::Dialog;
        // PORTS: `dialog.zig:21-24` `pub const Parent` — always `adw.AlertDialog`
        // under AD-3 (the `MessageDialog` arm is dropped).
        type ParentType = libadwaita::AlertDialog;
    }

    impl ObjectImpl for Dialog {}
    impl WidgetImpl for Dialog {}
    impl AdwDialogImpl for Dialog {}
    impl AdwAlertDialogImpl for Dialog {}
}

glib::wrapper! {
    /// Dialog is a simple abstraction over the `adw.AlertDialog` and
    /// `adw.MessageDialog` widgets, chosen at comptime based on the linked
    /// Adwaita version.
    ///
    /// Once we drop support for Adwaita <= 1.2, this can be fully removed
    /// and we can use `adw.AlertDialog` directly.
    ///
    /// PORTS: `dialog.zig:17` `pub const Dialog`. A subclassable base over
    /// `adw::AlertDialog`; see [`DialogImpl`] for how subclasses attach.
    pub struct Dialog(ObjectSubclass<imp::Dialog>)
        @extends libadwaita::AlertDialog, libadwaita::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

/// The subclass contract for [`Dialog`] — mirrors gtk4-rs's own `BinImpl` /
/// `AdwAlertDialogImpl` marker traits. A type that is `AdwAlertDialogImpl`
/// (hence `AdwDialogImpl` + `WidgetImpl`) can subclass `Dialog`. `Dialog` adds
/// no vfuncs of its own, so the trait body is empty and the `IsSubclassable`
/// impl just chains to `adw::AlertDialog` (which installs the `response` vfunc
/// the reference forwards at `dialog.zig:32-36`).
//
// Reference `virtual_methods.response` (dialog.zig:33-35):
// Forwarded from parent so subclasses can reference this
// directly. This will make it easier to remove Dialog in the future.
pub trait DialogImpl: AdwAlertDialogImpl {}

unsafe impl<T: DialogImpl> IsSubclassable<T> for Dialog {}
