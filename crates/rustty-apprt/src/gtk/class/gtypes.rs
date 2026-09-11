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
use rustty_config::value_types::GtkTitlebarStyle;
use rustty_vt::mouse::Shape;

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
///
/// Value names AND nicks are the Zig field names: `gobject.ext.defineEnum` sets
/// `.f_value_name = field.name, .f_value_nick = field.name`
/// (`zig-pkg/gobject-*/src/gobject2/ext.zig:429-435`). `glib::Enum`'s own defaults
/// (UpperCamel name, kebab nick) differ, so every variant spells both out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "GhosttyApprtClipboard")]
#[repr(i32)]
pub enum ClipboardEnum {
    /// PORTS: `apprt/structs.zig:60` `Clipboard.standard = 0`.
    #[default]
    #[enum_value(name = "standard", nick = "standard")]
    Standard = 0,
    /// PORTS: `apprt/structs.zig:61` `Clipboard.selection = 1`.
    #[enum_value(name = "selection", nick = "selection")]
    Selection = 1,
    /// PORTS: `apprt/structs.zig:62` `Clipboard.primary = 2`.
    #[enum_value(name = "primary", nick = "primary")]
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

/// A `#[glib::Enum]` mirror of [`Shape`] so the surface's `mouse-shape` property
/// carries the reference's enum GType instead of a bare `i32`.
///
/// PORTS: `terminal/mouse.zig:78-92` `Shape.getGObjectType` —
/// `gobject.ext.defineEnum(Shape, .{ .name = "GhosttyMouseShape" })` under
/// `app_runtime == .gtk`. `rustty-vt` cannot carry the registration (it has no
/// glib dependency, R3), so it lives with the other apprt-gated GTypes here.
/// Discriminants are taken FROM [`Shape`], and both conversions are exhaustive
/// matches, so a variant added to `Shape` does not build until it is added here.
/// Value names and nicks are the Zig field names (see [`ClipboardEnum`]); the
/// `gtypes_value_names_are_the_zig_field_names` test derives them from `Shape`'s
/// own variant names rather than from this list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "GhosttyMouseShape")]
#[repr(i32)]
pub enum MouseShapeEnum {
    #[enum_value(name = "default", nick = "default")]
    Default = Shape::Default as i32,
    #[enum_value(name = "context_menu", nick = "context_menu")]
    ContextMenu = Shape::ContextMenu as i32,
    #[enum_value(name = "help", nick = "help")]
    Help = Shape::Help as i32,
    #[enum_value(name = "pointer", nick = "pointer")]
    Pointer = Shape::Pointer as i32,
    #[enum_value(name = "progress", nick = "progress")]
    Progress = Shape::Progress as i32,
    #[enum_value(name = "wait", nick = "wait")]
    Wait = Shape::Wait as i32,
    #[enum_value(name = "cell", nick = "cell")]
    Cell = Shape::Cell as i32,
    #[enum_value(name = "crosshair", nick = "crosshair")]
    Crosshair = Shape::Crosshair as i32,
    #[enum_value(name = "text", nick = "text")]
    Text = Shape::Text as i32,
    #[enum_value(name = "vertical_text", nick = "vertical_text")]
    VerticalText = Shape::VerticalText as i32,
    #[enum_value(name = "alias", nick = "alias")]
    Alias = Shape::Alias as i32,
    #[enum_value(name = "copy", nick = "copy")]
    Copy = Shape::Copy as i32,
    #[enum_value(name = "move", nick = "move")]
    Move = Shape::Move as i32,
    #[enum_value(name = "no_drop", nick = "no_drop")]
    NoDrop = Shape::NoDrop as i32,
    #[enum_value(name = "not_allowed", nick = "not_allowed")]
    NotAllowed = Shape::NotAllowed as i32,
    #[enum_value(name = "grab", nick = "grab")]
    Grab = Shape::Grab as i32,
    #[enum_value(name = "grabbing", nick = "grabbing")]
    Grabbing = Shape::Grabbing as i32,
    #[enum_value(name = "all_scroll", nick = "all_scroll")]
    AllScroll = Shape::AllScroll as i32,
    #[enum_value(name = "col_resize", nick = "col_resize")]
    ColResize = Shape::ColResize as i32,
    #[enum_value(name = "row_resize", nick = "row_resize")]
    RowResize = Shape::RowResize as i32,
    #[enum_value(name = "n_resize", nick = "n_resize")]
    NResize = Shape::NResize as i32,
    #[enum_value(name = "e_resize", nick = "e_resize")]
    EResize = Shape::EResize as i32,
    #[enum_value(name = "s_resize", nick = "s_resize")]
    SResize = Shape::SResize as i32,
    #[enum_value(name = "w_resize", nick = "w_resize")]
    WResize = Shape::WResize as i32,
    #[enum_value(name = "ne_resize", nick = "ne_resize")]
    NeResize = Shape::NeResize as i32,
    #[enum_value(name = "nw_resize", nick = "nw_resize")]
    NwResize = Shape::NwResize as i32,
    #[enum_value(name = "se_resize", nick = "se_resize")]
    SeResize = Shape::SeResize as i32,
    #[enum_value(name = "sw_resize", nick = "sw_resize")]
    SwResize = Shape::SwResize as i32,
    #[enum_value(name = "ew_resize", nick = "ew_resize")]
    EwResize = Shape::EwResize as i32,
    #[enum_value(name = "ns_resize", nick = "ns_resize")]
    NsResize = Shape::NsResize as i32,
    #[enum_value(name = "nesw_resize", nick = "nesw_resize")]
    NeswResize = Shape::NeswResize as i32,
    #[enum_value(name = "nwse_resize", nick = "nwse_resize")]
    NwseResize = Shape::NwseResize as i32,
    #[enum_value(name = "zoom_in", nick = "zoom_in")]
    ZoomIn = Shape::ZoomIn as i32,
    #[enum_value(name = "zoom_out", nick = "zoom_out")]
    ZoomOut = Shape::ZoomOut as i32,
}

impl From<Shape> for MouseShapeEnum {
    fn from(value: Shape) -> Self {
        match value {
            Shape::Default => Self::Default,
            Shape::ContextMenu => Self::ContextMenu,
            Shape::Help => Self::Help,
            Shape::Pointer => Self::Pointer,
            Shape::Progress => Self::Progress,
            Shape::Wait => Self::Wait,
            Shape::Cell => Self::Cell,
            Shape::Crosshair => Self::Crosshair,
            Shape::Text => Self::Text,
            Shape::VerticalText => Self::VerticalText,
            Shape::Alias => Self::Alias,
            Shape::Copy => Self::Copy,
            Shape::Move => Self::Move,
            Shape::NoDrop => Self::NoDrop,
            Shape::NotAllowed => Self::NotAllowed,
            Shape::Grab => Self::Grab,
            Shape::Grabbing => Self::Grabbing,
            Shape::AllScroll => Self::AllScroll,
            Shape::ColResize => Self::ColResize,
            Shape::RowResize => Self::RowResize,
            Shape::NResize => Self::NResize,
            Shape::EResize => Self::EResize,
            Shape::SResize => Self::SResize,
            Shape::WResize => Self::WResize,
            Shape::NeResize => Self::NeResize,
            Shape::NwResize => Self::NwResize,
            Shape::SeResize => Self::SeResize,
            Shape::SwResize => Self::SwResize,
            Shape::EwResize => Self::EwResize,
            Shape::NsResize => Self::NsResize,
            Shape::NeswResize => Self::NeswResize,
            Shape::NwseResize => Self::NwseResize,
            Shape::ZoomIn => Self::ZoomIn,
            Shape::ZoomOut => Self::ZoomOut,
        }
    }
}

impl From<MouseShapeEnum> for Shape {
    fn from(value: MouseShapeEnum) -> Self {
        match value {
            MouseShapeEnum::Default => Self::Default,
            MouseShapeEnum::ContextMenu => Self::ContextMenu,
            MouseShapeEnum::Help => Self::Help,
            MouseShapeEnum::Pointer => Self::Pointer,
            MouseShapeEnum::Progress => Self::Progress,
            MouseShapeEnum::Wait => Self::Wait,
            MouseShapeEnum::Cell => Self::Cell,
            MouseShapeEnum::Crosshair => Self::Crosshair,
            MouseShapeEnum::Text => Self::Text,
            MouseShapeEnum::VerticalText => Self::VerticalText,
            MouseShapeEnum::Alias => Self::Alias,
            MouseShapeEnum::Copy => Self::Copy,
            MouseShapeEnum::Move => Self::Move,
            MouseShapeEnum::NoDrop => Self::NoDrop,
            MouseShapeEnum::NotAllowed => Self::NotAllowed,
            MouseShapeEnum::Grab => Self::Grab,
            MouseShapeEnum::Grabbing => Self::Grabbing,
            MouseShapeEnum::AllScroll => Self::AllScroll,
            MouseShapeEnum::ColResize => Self::ColResize,
            MouseShapeEnum::RowResize => Self::RowResize,
            MouseShapeEnum::NResize => Self::NResize,
            MouseShapeEnum::EResize => Self::EResize,
            MouseShapeEnum::SResize => Self::SResize,
            MouseShapeEnum::WResize => Self::WResize,
            MouseShapeEnum::NeResize => Self::NeResize,
            MouseShapeEnum::NwResize => Self::NwResize,
            MouseShapeEnum::SeResize => Self::SeResize,
            MouseShapeEnum::SwResize => Self::SwResize,
            MouseShapeEnum::EwResize => Self::EwResize,
            MouseShapeEnum::NsResize => Self::NsResize,
            MouseShapeEnum::NeswResize => Self::NeswResize,
            MouseShapeEnum::NwseResize => Self::NwseResize,
            MouseShapeEnum::ZoomIn => Self::ZoomIn,
            MouseShapeEnum::ZoomOut => Self::ZoomOut,
        }
    }
}

/// A `#[glib::Enum]` mirror of [`GtkTitlebarStyle`] so the window's
/// `titlebar-style` property carries the reference's enum GType.
///
/// PORTS: `config/Config.zig:9234-9246` `GtkTitlebarStyle.getGObjectType` —
/// `gobject.ext.defineEnum(GtkTitlebarStyle, .{ .name = "GhosttyGtkTitlebarStyle" })`.
/// Names and nicks as for [`ClipboardEnum`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "GhosttyGtkTitlebarStyle")]
#[repr(i32)]
pub enum GtkTitlebarStyleEnum {
    #[default]
    #[enum_value(name = "native", nick = "native")]
    Native = GtkTitlebarStyle::Native as i32,
    #[enum_value(name = "tabs", nick = "tabs")]
    Tabs = GtkTitlebarStyle::Tabs as i32,
}

impl From<GtkTitlebarStyle> for GtkTitlebarStyleEnum {
    fn from(value: GtkTitlebarStyle) -> Self {
        match value {
            GtkTitlebarStyle::Native => Self::Native,
            GtkTitlebarStyle::Tabs => Self::Tabs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `lower_snake` of an UpperCamel Rust variant name: the Zig field name the
    /// reference's `defineEnum` registers as both value name and nick.
    fn zig_field_name(rust: &str) -> String {
        let mut out = String::new();
        for (i, c) in rust.chars().enumerate() {
            if c.is_ascii_uppercase() {
                if i > 0 {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
            } else {
                out.push(c);
            }
        }
        out
    }

    fn values(ty: glib::Type) -> Vec<(i32, String, String)> {
        let class = glib::EnumClass::with_type(ty).expect("an enum GType");
        class
            .values()
            .iter()
            .map(|v| (v.value(), v.name().to_string(), v.nick().to_string()))
            .collect()
    }

    /// S54-F3 / S58-F4: the three enum GTypes carry the reference's registered
    /// names and `defineEnum`'s value names/nicks. The expected rows are derived
    /// from the SOURCE enums (`Shape`'s variants through `Debug`, the discriminant
    /// through the reverse conversion), not from the mirror, so a mistyped
    /// `#[enum_value]` fails. Display-free: registering an enum GType needs no GTK.
    #[test]
    fn gtypes_value_names_are_the_zig_field_names() {
        use glib::prelude::StaticType;

        assert_eq!(MouseShapeEnum::static_type().name(), "GhosttyMouseShape");
        let got = values(MouseShapeEnum::static_type());
        assert_eq!(got.len(), 34);
        for (value, name, nick) in &got {
            // SAFETY: `value` was read from this enum's own GEnumClass, so it is a
            // registered discriminant of `MouseShapeEnum`.
            let mirror: MouseShapeEnum = unsafe { glib::translate::from_glib(*value) };
            let shape = Shape::from(mirror);
            assert_eq!(*value, shape as i32, "discriminants follow `Shape`");
            let want = zig_field_name(&format!("{shape:?}"));
            assert_eq!((name, nick), (&want, &want), "value {value}");
        }

        assert_eq!(ClipboardEnum::static_type().name(), "GhosttyApprtClipboard");
        assert_eq!(
            values(ClipboardEnum::static_type()),
            [
                (0, "standard".into(), "standard".into()),
                (1, "selection".into(), "selection".into()),
                (2, "primary".into(), "primary".into()),
            ]
        );

        assert_eq!(
            GtkTitlebarStyleEnum::static_type().name(),
            "GhosttyGtkTitlebarStyle"
        );
        assert_eq!(
            values(GtkTitlebarStyleEnum::static_type()),
            [
                (0, "native".into(), "native".into()),
                (1, "tabs".into(), "tabs".into()),
            ]
        );
    }
}
