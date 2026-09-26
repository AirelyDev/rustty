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
//! allocator. The non-allocating readers are [`get_bool`] (`Surface.init`
//! reads `gtk-enable-primary-paste`, `surface.zig:1816`) and [`get_c_int`]
//! (`gtk-xft-dpi`); the allocating reader is [`get_alloc`] (parity tail S-8d,
//! N282 — it has no caller in the reference GTK tree at the pin, and none here).

/// GTK Settings keys with well-defined types.
///
/// PORTS: `gsettings.zig:6-9` `pub const Key`. The `@"…"`-quoted Zig identifiers (the literal GObject
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

    /// Returns true if this setting type requires memory allocation.
    /// Types that do not need allocation must be explicitly marked.
    ///
    /// PORTS: `gsettings.zig:30-36` `requiresAllocation()`. `bool`/`c_int` do
    /// not; every other type does.
    #[must_use]
    pub const fn requires_allocation(self) -> bool {
        match self.value_type() {
            KeyType::Bool | KeyType::CInt => false,
            KeyType::Str => true,
        }
    }
}

/// Reads a GTK setting for non-allocating types.
/// Automatically uses XDG Desktop Portal in Flatpak environments.
/// Returns null if the setting is unavailable.
///
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

/// Reads a GTK setting, allocating memory if necessary.
/// Automatically uses XDG Desktop Portal in Flatpak environments.
/// Caller must free returned memory with the provided allocator.
/// Returns null if the setting is unavailable.
///
/// PORTS: `gsettings.zig:50-57` `getAlloc` over `getImpl` (`:59-77`). Like the
/// reference it accepts EVERY key: `bool`/`c_int` keys read straight out of the
/// `GValue` (`:67-68`), and the `[]const u8` key duplicates the C string's bytes
/// (`:69-74`: `const ptr = value.getString() orelse break :blk null; ... try
/// alloc.dupe(u8, str)`). Returns `None` when no default `Settings` is available
/// (`:55` `orelse return null`) or the string is NULL.
///
/// PORT NOTE: the value is read as the raw `GValue` (`property_value` +
/// `g_value_get_string`), so the string arm returns the exact bytes the
/// reference's `std.mem.span` sees — a Rust `String` read would panic on
/// non-UTF-8 (memory `zig-byteslice-to-rust-rawbytes`). "Caller must free" is
/// discharged by `Vec`'s drop, and the `std.mem.Allocator` parameter collapses
/// with it; the only reference error is `alloc.dupe`'s `OutOfMemory`, which the
/// Rust global allocator turns into an abort, so the error union collapses too.
/// The reference's comptime return type `?key.Type()` is the [`SettingValue`]
/// enum.
#[must_use]
pub fn get_alloc(key: Key) -> Option<SettingValue> {
    use glib::prelude::ObjectExt as _;
    use glib::translate::ToGlibPtr as _;

    let settings = gtk4::Settings::default()?;
    match key.value_type() {
        // `bool => value.getInt() != 0` (`:67`).
        KeyType::Bool => Some(SettingValue::Bool(settings.property::<bool>(key.name()))),
        // `c_int => value.getInt()` (`:68`).
        KeyType::CInt => Some(SettingValue::CInt(settings.property::<i32>(key.name()))),
        KeyType::Str => {
            let value = settings.property_value(key.name());
            // SAFETY: `value` is a live, initialized GValue of the property's own
            // type (`G_TYPE_STRING` for this key); `g_value_get_string` returns
            // its borrowed, NUL-terminated contents or NULL, valid while `value`
            // lives. The bytes are copied out before `value` drops.
            let ptr = unsafe { glib::gobject_ffi::g_value_get_string(value.to_glib_none().0) };
            if ptr.is_null() {
                return None;
            }
            // SAFETY: non-NULL and NUL-terminated per the above.
            let bytes = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_bytes().to_vec();
            Some(SettingValue::Str(bytes))
        }
    }
}

/// A setting value as [`get_alloc`] returns it — the reference's `?key.Type()`
/// (`bool` / `c_int` / `[]const u8`), reified as data like [`KeyType`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    /// `bool` (`gsettings.zig:67`).
    Bool(bool),
    /// `c_int` (`gsettings.zig:68`).
    CInt(i32),
    /// `[]const u8` (`gsettings.zig:69-74`): raw bytes, never re-validated as UTF-8.
    Str(Vec<u8>),
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
