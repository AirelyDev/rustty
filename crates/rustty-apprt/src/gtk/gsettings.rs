//! rustty-apprt::gtk::gsettings
//!
//! PORTS: ghostty `src/apprt/gtk/gsettings.zig` (101 loc, 4 tests @ `:79,:85,
//! :91,:97`). Typed reads of well-known GTK `GSettings`-backed `gtk.Settings`
//! keys.
//!
//! # The comptime dispatch (PREPORT_GTK §3, `gsettings.zig:11-42`)
//!
//! The oracle keys each of `Type()` / `GValueType()` / `requiresAllocation()`
//! off a `comptime self: Key` and `switch`es to a **Zig type** (`bool`, `c_int`,
//! `[]const u8`) — a compile-time type computation Rust cannot spell as a value.
//! rustty reifies each of those type computations as data: [`KeyType`] mirrors
//! `Type()` (`gsettings.zig:11-17`), [`GValueType`] mirrors `GValueType()`
//! (`:19-26`), and [`Key::requires_allocation`] mirrors `requiresAllocation()`
//! (`:30-36`). The `@tagName(key)` GObject-property name (`:64,:98-100`) becomes
//! [`Key::name`]. The four oracle tests port 1:1 over this reified data.
//!
//! # The reader
//!
//! The oracle's `get`/`getAlloc`/`getImpl` (`:42-77`) split on
//! `requiresAllocation()` at comptime: non-allocating keys (`bool`/`c_int`) read
//! straight out of a `GValue`; the allocating key (`gtk-font-name`) needs an
//! allocator. Only the non-allocating **bool** path is consumed by the tier so
//! far — `Surface.init` reads `gtk-enable-primary-paste` (`surface.zig:1816`) —
//! so [`get_bool`] is the ported reader; the allocating `getAlloc` path lands
//! with its first consumer (none at this pin).

/// PORTS: `gsettings.zig:6-9` `pub const Key`. GTK Settings keys with
/// well-defined types. The `@"…"`-quoted Zig identifiers (the literal GObject
/// property names) become ordinary Rust variants; [`Key::name`] recovers the
/// wire name (`@tagName`, `gsettings.zig:64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// `gtk-enable-primary-paste` (`gsettings.zig:7`).
    GtkEnablePrimaryPaste,
    /// `gtk-xft-dpi` (`gsettings.zig:8`).
    GtkXftDpi,
    /// `gtk-font-name` (`gsettings.zig:9`).
    GtkFontName,
}

/// PORTS: `gsettings.zig:11-17` `Key.Type()` — the Zig type each key reads as,
/// reified as a value (`bool` / `c_int` / `[]const u8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    /// `bool` (`gsettings.zig:13`).
    Bool,
    /// `c_int` (`gsettings.zig:14`).
    CInt,
    /// `[]const u8` (`gsettings.zig:15`).
    Str,
}

/// PORTS: `gsettings.zig:19-26` `Key.GValueType()` — the `GValue`-carried type,
/// reified as a value. `bool`/`c_int` both marshal through a `c_int` `GValue`
/// (`:21-22`); `[]const u8` marshals through a nullable C string
/// (`?[*:0]const u8`, `:23`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GValueType {
    /// `c_int` (`gsettings.zig:21-22`).
    CInt,
    /// `?[*:0]const u8` (`gsettings.zig:23`).
    OptCStr,
}

impl Key {
    /// PORTS: `@tagName(key)` (`gsettings.zig:64,98-100`) — the literal GObject
    /// property name passed to `gtk.Settings`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Key::GtkEnablePrimaryPaste => "gtk-enable-primary-paste",
            Key::GtkXftDpi => "gtk-xft-dpi",
            Key::GtkFontName => "gtk-font-name",
        }
    }

    /// PORTS: `gsettings.zig:11-17` `Type()`.
    #[must_use]
    pub const fn value_type(self) -> KeyType {
        match self {
            Key::GtkEnablePrimaryPaste => KeyType::Bool,
            Key::GtkXftDpi => KeyType::CInt,
            Key::GtkFontName => KeyType::Str,
        }
    }

    /// PORTS: `gsettings.zig:19-26` `GValueType()`. Dispatches on
    /// [`Key::value_type`] exactly as the oracle switches on `self.Type()`.
    #[must_use]
    pub const fn gvalue_type(self) -> GValueType {
        match self.value_type() {
            // `bool => c_int`, `c_int => c_int` (`gsettings.zig:21-22`).
            KeyType::Bool | KeyType::CInt => GValueType::CInt,
            // `[]const u8 => ?[*:0]const u8` (`gsettings.zig:23`).
            KeyType::Str => GValueType::OptCStr,
        }
    }

    /// PORTS: `gsettings.zig:30-36` `requiresAllocation()`. "Types that do not
    /// need allocation must be explicitly marked." `bool`/`c_int` do not; every
    /// other type does.
    #[must_use]
    pub const fn requires_allocation(self) -> bool {
        match self.value_type() {
            KeyType::Bool | KeyType::CInt => false,
            KeyType::Str => true,
        }
    }
}

/// PORTS: `gsettings.zig:42-48` `get` (the non-allocating reader) specialized to
/// the sole boolean key the tier consumes. Reads `gtk.Settings.getDefault()`
/// (`:46`) and, per `getImpl` (`:66-67`), interprets the value as `!= 0`. Returns
/// `None` when no default `Settings` is available (`:46` `orelse return null`).
///
/// The oracle's comptime `@compileError` guard against calling `get` on an
/// allocating key (`:43-45`) survives as the [`Key::requires_allocation`]
/// `debug_assert!` below — a bool key never allocates, so this reader is only
/// valid for non-allocating keys.
#[must_use]
pub fn get_bool(key: Key) -> Option<bool> {
    use glib::prelude::ObjectExt as _;

    debug_assert!(
        !key.requires_allocation(),
        "get_bool is a non-allocating reader; {} requires allocation",
        key.name()
    );
    debug_assert!(
        matches!(key.value_type(), KeyType::Bool),
        "get_bool reads a bool key; {} is not bool",
        key.name()
    );

    let settings = gtk4::Settings::default()?;
    // `gsettings.zig:66-67`: `bool => value.getInt() != 0`. gtk4-rs marshals the
    // boolean `gtk.Settings` property straight to a Rust `bool`.
    Some(settings.property::<bool>(key.name()))
}

/// PORTS: `gsettings.zig:42-48,66-67` `get`/`getImpl` specialized to the
/// `c_int` (non-allocating) keys the tier consumes — `Surface.getContentScale`
/// reads `gtk-xft-dpi` (`surface.zig:1553`). Reads `gtk.Settings.getDefault()`
/// (`:46`) and returns the value straight out of the `GValue` (`:66-67`
/// `c_int => value.getInt()`). Returns `None` when no default `Settings` is
/// available (`:46` `orelse return null`).
#[must_use]
pub fn get_c_int(key: Key) -> Option<i32> {
    use glib::prelude::ObjectExt as _;

    debug_assert!(
        !key.requires_allocation(),
        "get_c_int is a non-allocating reader; {} requires allocation",
        key.name()
    );
    debug_assert!(
        matches!(key.value_type(), KeyType::CInt),
        "get_c_int reads a c_int key; {} is not c_int",
        key.name()
    );

    let settings = gtk4::Settings::default()?;
    Some(settings.property::<i32>(key.name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: `gsettings.zig:79-83` `test "Key.Type returns correct types"`.
    /// Display-free: pure data over the reified [`KeyType`].
    #[test]
    fn gsettings_key_type() {
        assert_eq!(Key::GtkEnablePrimaryPaste.value_type(), KeyType::Bool);
        assert_eq!(Key::GtkXftDpi.value_type(), KeyType::CInt);
        assert_eq!(Key::GtkFontName.value_type(), KeyType::Str);
    }

    /// PORTS: `gsettings.zig:85-89` `test "Key.requiresAllocation identifies
    /// allocating types"`.
    #[test]
    fn gsettings_key_requires_allocation() {
        assert!(!Key::GtkEnablePrimaryPaste.requires_allocation());
        assert!(!Key::GtkXftDpi.requires_allocation());
        assert!(Key::GtkFontName.requires_allocation());
    }

    /// PORTS: `gsettings.zig:91-95` `test "Key.GValueType returns correct GObject
    /// types"`.
    #[test]
    fn gsettings_key_gvalue_type() {
        assert_eq!(Key::GtkEnablePrimaryPaste.gvalue_type(), GValueType::CInt);
        assert_eq!(Key::GtkXftDpi.gvalue_type(), GValueType::CInt);
        assert_eq!(Key::GtkFontName.gvalue_type(), GValueType::OptCStr);
    }

    /// PORTS: `gsettings.zig:97-101` `test "@tagName returns correct GTK property
    /// names"`.
    #[test]
    fn gsettings_key_tag_names() {
        assert_eq!(
            Key::GtkEnablePrimaryPaste.name(),
            "gtk-enable-primary-paste"
        );
        assert_eq!(Key::GtkXftDpi.name(), "gtk-xft-dpi");
        assert_eq!(Key::GtkFontName.name(), "gtk-font-name");
    }
}
