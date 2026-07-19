//! rustty-apprt::gtk::class::gtypes
//!
//! The apprt-api GObject types the reference registers **in the apprt layer but
//! gated on `app_runtime == .gtk`**, deferred by `rustty-apprt-api` to this GTK
//! backend (each apprt-api type carries a `PORT NOTE: reference `getGObjectType`
//! ... is deferred to rustty-apprt's gtk backend`).
//!
//! These are boxed/enum newtypes so an apprt value can cross a GObject
//! property/signal boundary as a `glib::Value`. They mirror the established boxed
//! idiom (`split_tree.rs` `Tree`, `clipboard_confirmation_dialog.rs`
//! `ClipboardRequestBoxed`, `surface_child_exited.rs` `ChildExited`,
//! `surface.rs` `Size`): a `#[derive(glib::Boxed)]` tuple newtype whose
//! `#[boxed_type(name = ...)]` reproduces the reference's registered GType name.
//!
//! Registration is lazy (gtk4-rs interns the GType on first `static_type()` /
//! `ensure_type()`), exactly as the reference's comptime `getGObjectType` resolves
//! on first use by the property/signal accessor that carries it. The consuming
//! accessors themselves are deferred to their own tiers; these registrations
//! resolve the apprt-api-side deferral so those accessors have a GType to name.

use gtk4::glib;

use rustty_apprt_api::action::InitialSize;
use rustty_apprt_api::Clipboard;

/// A `#[glib::Enum]` mirror of [`Clipboard`] so it can cross a GObject boundary
/// as a `glib::Value` backed by a `g_param_spec_enum` accessor.
///
/// PORTS: `apprt/structs.zig` `Clipboard.getGObjectType` — `gobject.ext.defineEnum(
/// Clipboard, .{ .name = "GhosttyApprtClipboard" })`, gated on `app_runtime ==
/// .gtk` (see the `rustty-apprt-api` `Clipboard` PORT NOTE). The reference registers
/// this as an **enum** GType (`defineEnum`), not a boxed GType — mirrored here with
/// the `glib::Enum` idiom (`close_confirmation_dialog.rs` `Target`) so the
/// fundamental GType KIND matches the reference (a future clipboard property will
/// declare `g_param_spec_enum`, which mismatches a boxed GType). The GTK backing is
/// `c_int`/`i32` per the reference's `Backing = c_int` under `.gtk`; the
/// discriminants (0/1/2) mirror [`Clipboard`] verbatim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "GhosttyApprtClipboard")]
#[repr(i32)]
pub enum ClipboardEnum {
    /// PORTS: `apprt/structs.zig:60` `Clipboard.standard = 0`.
    #[default]
    Standard = 0,
    /// PORTS: `apprt/structs.zig:61` `Clipboard.selection = 1`.
    Selection = 1,
    /// PORTS: `apprt/structs.zig:62` `Clipboard.primary = 2`.
    Primary = 2,
}

impl From<Clipboard> for ClipboardEnum {
    fn from(value: Clipboard) -> Self {
        match value {
            Clipboard::Standard => Self::Standard,
            Clipboard::Selection => Self::Selection,
            Clipboard::Primary => Self::Primary,
        }
    }
}

impl From<ClipboardEnum> for Clipboard {
    fn from(value: ClipboardEnum) -> Self {
        match value {
            ClipboardEnum::Standard => Self::Standard,
            ClipboardEnum::Selection => Self::Selection,
            ClipboardEnum::Primary => Self::Primary,
        }
    }
}

/// A `#[glib::Boxed]` newtype for [`InitialSize`] so it can back the initial-size
/// property/action GObject boundary.
///
/// PORTS: `apprt/action.zig` `InitialSize.getGObjectType` — `gobject.ext.defineBoxed(
/// ..., "GhosttyApprtInitialSize")`, gated on `app_runtime == .gtk` (see the
/// `rustty-apprt-api` `InitialSize` PORT NOTE at `action.rs`).
#[derive(Clone, Copy, Debug, glib::Boxed)]
#[boxed_type(name = "GhosttyApprtInitialSize", nullable)]
pub struct InitialSizeBoxed(pub InitialSize);
