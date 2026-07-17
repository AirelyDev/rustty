//! rustty-apprt::gtk::ext
//!
//! PORTS: ghostty `src/apprt/gtk/ext.zig` (70 loc, 1 `test{}` @ `:67`
//! `_ = actions; _ = slice;`). "Extensions/helpers for GTK objects … Zig-friendly
//! wrappers around existing GTK functionality, rather than complex new helpers."
//! (`ext.zig:1-4`). The rustty analogues are correspondingly thin — most of what
//! the oracle wraps for Zig-friendliness gtk4-rs already exposes idiomatically.
//!
//! # Scope at pin `4749c4e9`
//!
//! The pinned `ext.zig` defines exactly: `boxedCopy` (`:19`), `boxedFree`
//! (`:25`), `listFind` (`:34`), `getAncestor` (`:52`), `gValueHolds` (`:61`),
//! and re-exports `actions` + `StringList`. This module ports the reusable
//! helpers plus the two submodules. See the report for the `listFind` /
//! `ancestor_while` disposition (both are noted there).

pub mod actions;
pub mod slice;

pub use slice::StringList;

use glib::prelude::*;
use glib::translate::IntoGlib;
use gtk4::prelude::WidgetExt;

/// PORTS: `ext.zig:19` `boxedCopy`. "Wrapper around `gobject.boxedCopy` to copy
/// a boxed type `T`." The oracle threads `T.getGObjectType()`; the gtk4-rs
/// analogue is [`StaticType::static_type`], which for a `#[derive(glib::Boxed)]`
/// type is exactly the registered `GBoxed` `GType`. Returns a freshly allocated
/// pointer the caller owns (free it with [`boxed_free`]).
///
/// `ptr` must be a valid, non-null pointer to a live `T` boxed instance — the
/// same contract as the oracle's `*const T` parameter.
#[must_use]
pub fn boxed_copy<T: StaticType>(ptr: *const T) -> *mut T {
    unsafe {
        glib::gobject_ffi::g_boxed_copy(
            T::static_type().into_glib(),
            ptr as glib::ffi::gconstpointer,
        ) as *mut T
    }
}

/// PORTS: `ext.zig:25` `boxedFree`. "Wrapper around `gobject.boxedFree` to free a
/// boxed type `T`." Null-guarded exactly like the oracle's `ptr: ?*T` (`:26`
/// `if (ptr) |p|`).
pub fn boxed_free<T: StaticType>(ptr: *mut T) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        glib::gobject_ffi::g_boxed_free(T::static_type().into_glib(), ptr as glib::ffi::gpointer);
    }
}

/// PORTS: `ext.zig:52` `getAncestor`. "Wrapper around `gtk.Widget.getAncestor`
/// to get the widget ancestor of the given type `T`, or null if it doesn't
/// exist." The oracle asserts the final cast is non-null (`:56` `.?`) because
/// `getAncestor` already filtered to that `GType`; the `.expect` here preserves
/// that assertion.
#[must_use]
pub fn ancestor<T: IsA<gtk4::Widget>>(widget: &impl IsA<gtk4::Widget>) -> Option<T> {
    widget
        .ancestor(T::static_type())
        .map(|a| a.downcast::<T>().expect("getAncestor guarantees the GType"))
}

/// PORTS: `ext.zig:61` `gValueHolds`. "equivalent to GTK's `G_VALUE_HOLDS()`
/// macro but Zig's C translator does not like it" (`:59-60`). That Zig-specific
/// reason does not apply to Rust: gtk4-rs already binds `G_VALUE_HOLDS` as
/// [`glib::Value::is_type`] (`g_type_check_value_holds` + the fundamental-type
/// fast path). Kept as a named wrapper so the oracle's call sites port 1:1.
#[must_use]
pub fn g_value_holds(value: &glib::Value, g_type: glib::Type) -> bool {
    value.is_type(g_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: `ext.zig:67` `test { _ = actions; _ = slice; }` — a compile-touch
    /// that forces the submodules and this module's public helpers to type-check.
    #[test]
    fn ext_ref_all_decls() {
        // reference: `_ = actions; _ = slice;`
        let _: fn(&str) -> bool = actions::g_action_name_is_valid;
        let _ = StringList::new(&[]);

        // this module's own reusable helpers (refAllDecls analogue)
        let _: fn(*const StringList) -> *mut StringList = boxed_copy::<StringList>;
        let _: fn(*mut StringList) = boxed_free::<StringList>;
        let _: fn(&glib::Value, glib::Type) -> bool = g_value_holds;
    }
}
