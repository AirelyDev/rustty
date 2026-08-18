//! rustty-apprt::gtk::weak_ref
//!
//! PORTS: ghostty `src/apprt/gtk/weak_ref.zig` (44 loc, 1 `test WeakRef` @ `:35`).
//!
//! `weak_ref.zig` is "a lightweight wrapper around `gobject.WeakRef` to make it
//! type-safe to hold a single type of value" (`weak_ref.zig:5-6`). gtk4-rs
//! already ships that type-safe wrapper as [`glib::WeakRef<T>`]; this module is
//! the thin newtype that carries the ORACLE'S NAMES (`empty`/`set`/`get`) so the
//! reference's call sites port 1:1, plus the Invariant-4 lifetime documentation.
//!
//! # Invariant 4 — the `get()`/`unref()` pairing (PREPORT_GTK §2)
//!
//! `weak_ref.zig`'s `get()` returns a **strong** reference: it calls
//! `gobject.WeakRef.get` which "increases the reference count by one"
//! (`weak_ref.zig:26`). The reference therefore pairs every `.get()` with a
//! `defer unref` (its own test does exactly this: `ref.get().?.unref()`,
//! `weak_ref.zig:41`).
//!
//! In rustty that pairing becomes **scope drop**: [`WeakRef::get`] returns an
//! owned `T` whose `Drop` unrefs the object. So the oracle's `defer unref`
//! is discharged by the returned value going out of scope — there is no
//! `unref` to call, and there must not be. Every reference `.get()` site that
//! ported a `defer <v>.unref()` (`weak_ref.zig:27-31`) drops that defer and
//! relies on the returned `T`'s destructor instead. Holding the returned `T`
//! keeps the object alive exactly as long as the binding lives.

use glib::object::ObjectType;

/// A type-safe weak reference to a single GObject type `T`.
///
/// PORTS: `weak_ref.zig:7` `pub fn WeakRef(comptime T: type) type`. The generic
/// `struct { ref: gobject.WeakRef }` becomes a newtype over [`glib::WeakRef<T>`]
/// — gtk4-rs's own type-safe `GWeakRef` wrapper, which the reference
/// hand-rolls the type-safety for on top of the untyped `gobject.WeakRef`.
#[derive(Debug)]
pub struct WeakRef<T: ObjectType>(glib::WeakRef<T>);

impl<T: ObjectType> WeakRef<T> {
    /// PORTS: `weak_ref.zig:13` `pub const empty: Self = .{}` — a zero-initialized
    /// (`std.mem.zeroes`) `gobject.WeakRef`. gtk4-rs's [`glib::WeakRef::new`]
    /// `g_weak_ref_init`s an empty weak ref, which `upgrade` returns `None` from
    /// until an object is `set`.
    #[must_use]
    pub fn empty() -> Self {
        Self(glib::WeakRef::new())
    }

    /// PORTS: `weak_ref.zig:17` `set`. "Set the weak reference to the given
    /// object. This will not increase the reference count of the object."
    /// (`weak_ref.zig:15-16`). gtk4-rs's `set` is the same `g_weak_ref_set`;
    /// passing `None` clears it (the reference's `else` arm, `:21`).
    pub fn set(&self, v: Option<&T>) {
        self.0.set(v);
    }

    /// Release this weak reference.
    ///
    /// You MUST call this before the memory holding this struct is freed,
    /// which in practice means from the owner's `dispose`. The target keeps
    /// a pointer to this `GWeakRef` so that it can clear it when the target
    /// is finalized; if this memory is gone by then, the target walks into
    /// whatever now occupies it. That is an invalid read at best, and can
    /// hang: the target takes a lock inside each registered weak ref, and
    /// reused memory with the low bit set is a lock nothing will release.
    ///
    /// `set(null)` also unregisters and remains valid. This exists so the
    /// requirement has a name at the use site rather than looking like an
    /// ordinary assignment.
    //
    // PORTS: `weak_ref.zig` `deinit` (1dbc8ca30). VERIFICATION (recorded per the
    // assessment's warning against a blind copy): rustty's `WeakRef` wraps
    // gtk4-rs `glib::WeakRef<T>`, which is `Pin<Box<gobject_ffi::GWeakRef>>`
    // WITH a `Drop` impl that calls `g_weak_ref_clear`
    // (glib-0.22.8 `src/object.rs:3510,3563-3568`). Two consequences:
    //  1. The upstream memory hazard the doc above describes cannot occur here:
    //     the `GWeakRef` cell is its own heap allocation, and dropping the
    //     wrapper always `g_weak_ref_clear`s it before the box is freed.
    //  2. The reference body (`self.ref.clear()` = `g_weak_ref_clear`) must NOT
    //     be blind-copied: gtk4-rs does not expose `clear` precisely because its
    //     `Drop` owns that call, and clearing out from under it would be a
    //     double-clear. The faithful mapping is `set(None)` — `g_weak_ref_set
    //     (NULL)` — which unregisters the ref NOW (the teardown semantic the
    //     reference names) and leaves the cell valid for the wrapper's `Drop`.
    // The four reference `dispose` call sites (`application config_errors_dialog`,
    // `command_palette` jump surface, `split_tree last_focused`, `window
    // command_palette`) port as `deinit()` calls so the requirement keeps its
    // name at the use sites.
    pub fn deinit(&self) {
        self.set(None);
    }

    /// PORTS: `weak_ref.zig:27` `get`. "Get a strong reference to the object, or
    /// null if the object has been finalized. This increases the reference count
    /// by one." (`weak_ref.zig:25-26`) — Invariant 4. The returned `T` owns that
    /// strong reference; its `Drop` discharges the reference's `defer unref` (see
    /// the module doc).
    #[must_use]
    pub fn get(&self) -> Option<T> {
        self.0.upgrade()
    }
}

impl<T: ObjectType> Default for WeakRef<T> {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: `weak_ref.zig:35` `test WeakRef`.
    ///
    /// The oracle exercises the strong-ref-on-`get` contract with a
    /// `gtk.TextBuffer`. gtk4-rs guards `TextBuffer::new` with
    /// `assert_initialized_main_thread!` (gtk4-0.11.4
    /// `src/auto/text_buffer.rs`), which would force a display dependency the
    /// oracle's headless test does not have. A bare `glib::Object` is an equally
    /// refcounted `GObject` and drives the IDENTICAL [`WeakRef`] code path with
    /// no `gtk_init`, so this stays in the normal (non-`--ignored`) gate exactly
    /// as the reference test does.
    ///
    /// The Invariant-4 pairing is visible in the shape: `get()` returns an owned
    /// `T` used only as a temporary, so the strong ref it took is released at the
    /// end of the statement (the oracle's `ref.get().?.unref()` becomes
    /// `weak.get().is_some()`). Dropping the last owner (`obj`) then finalizes,
    /// and `get()` returns `None`.
    #[test]
    fn weak_ref_get_returns_strong_ref() {
        let weak: WeakRef<glib::Object> = WeakRef::empty();
        let obj = glib::Object::new::<glib::Object>();
        weak.set(Some(&obj));

        // `get()` hands back a strong ref (Invariant 4); the temporary drops here,
        // releasing that ref and leaving `obj` as the sole owner.
        assert!(weak.get().is_some());

        // drop the last strong reference -> the object is finalized
        drop(obj);
        assert!(weak.get().is_none());
    }
}
