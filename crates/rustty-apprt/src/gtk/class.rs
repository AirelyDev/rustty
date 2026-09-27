//! rustty-apprt::gtk::class
//!
//! This files contains all the GObject classes for the GTK apprt
//! along with helpers to work with them.
//!
//! PORTS: ghostty `src/apprt/gtk/class.zig` (365 loc, 1 `test{}` @ `:365`
//! `refAllDecls`). This file is the **subclass-idiom bridge**: ghostty's
//! `class.zig` hand-rolls GObject subclassing with a single generic mixin
//! `Common(Self, Private)` (`class.zig:16-363`); gtk4-rs does the same job with
//! the `#[glib::object_subclass]` + `CompositeTemplate` + `glib::Properties`
//! macro triad. **There is no reusable Rust `Common` to port** — every method
//! the mixin injects is already provided, per class, by the gtk4-rs framework.
//! What survives is (a) the porter's MAP from each `Common` construct to its
//! gtk4-rs replacement, encoded below as live, test-checked data ([`IDIOM_MAP`]),
//! and (b) the transfer-mode and ABI notes the 22 class ports (L5.1+) must honor
//! when they re-express these constructs.
//!
//! # The map (PREPORT_GTK §3, `class.zig` line-by-line)
//!
//! | `class.zig`                          | site        | gtk4-rs replacement |
//! |--------------------------------------|-------------|---------------------|
//! | `Common(Self, Private)`              | `:16`       | `#[glib::object_subclass]` on the `ObjectSubclass` impl; `Private` → its instance struct |
//! | `as(self, T)`                        | `:25`       | `.upcast()` / `.upcast_ref()` — safe widening via `IsA<T>` |
//! | *(unsafe cast)*                      | `:23-24`    | `.dynamic_cast()` / `.downcast()` — the "noisier" unsafe path the oracle points at `gobject.ext.cast` |
//! | `ref` / `refSink` / `unref`          | `:30,36,41` | automatic — the gtk4-rs wrapper's `Clone`/`Drop` refcount; floating refs are sunk by `glib::Object::new` (Invariant 12) |
//! | `private(self)`                       | `:47`       | `self.imp()` (the `ObjectSubclassExt` accessor) |
//! | `getClass` / `f_g_class`             | `:86-89`    | **does NOT port** — raw `TypeInstance` layout poking; gtk4-rs exposes the class vtable only through the safe `*Impl` traits **[R1 waiver]** |
//! | `defineVirtualMethod(name)`          | `:99`       | override the method in the subclass's `*Impl` trait (e.g. `WidgetImpl::realize`) **[R1 waiver]** |
//! | `privateShallowFieldAccessor`        | `:165`      | `#[property(get, set)]` on a primitive field (shallow copy) |
//! | `privateBoxedFieldAccessor`          | `:187`      | `#[property]` on a boxed field — read allocates, write frees prior (transfer: get `.none`, set `.full`) |
//! | `privateObjFieldAccessor`            | `:230`      | `#[property]` on a GObject field — read refs, write unrefs prior (transfer: get `.none`, set `.full`) |
//! | `privateStringFieldAccessor`         | `:271`      | `#[property]` on a `Option<String>` field — getter `.none` (caller frees the copy), setter `.full` (takes ownership) |
//! | `Class.bindTemplateChildPrivate`     | `:313`      | `#[template_child]` field in the `CompositeTemplate` derive |
//! | `Class.bindTemplateCallback` + guard | `:330-352`  | `#[template_callback]` fn — the "return `c_int` not `bool`" ABI guard becomes **`-> bool`** (glib-rs marshals the return through a `GValue`, which must be `G_TYPE_BOOLEAN`; see [`TemplateCallbackReturn`]) |
//!
//! # Transfer-mode fidelity (the fidelity bug surface, PREPORT §5.1)
//!
//! The four `private*FieldAccessor` helpers encode EXACT ownership transfer
//! (`.none` = borrow, `.full` = take/give ownership). When a class port maps one
//! onto a `glib::Properties` field, the transfer mode MUST be preserved 1:1:
//!
//! * boxed / obj / string getters are **`.none`** (`class.zig:299` `getter_transfer`)
//!   — GTK hands the caller a fresh copy / a new strong ref that the CALLER owns.
//! * their setters are **`.full`** (`class.zig:301` `setter_transfer`) — GTK takes
//!   ownership of the passed value and the accessor frees/unrefs the prior field.
//!
//! Getting this backwards is a leak (double-owned) or a use-after-free
//! (double-freed); it is silent at compile time, which is why it is spelled out
//! here for every class port to check against.

pub mod application;
pub mod clipboard_confirmation_dialog;
pub mod close_confirmation_dialog;
pub mod command_palette;
pub mod config;
pub mod config_errors_dialog;
pub mod debug_warning;
pub mod dialog;
pub mod global_shortcuts;
pub mod gtypes;
pub mod imgui_widget;
pub mod inspector_widget;
pub mod inspector_window;
pub mod key_state_overlay;
pub mod resize_overlay;
pub mod search_overlay;
pub mod split_tree;
pub mod surface;
pub mod surface_child_exited;
pub mod surface_scrolled_window;
pub mod tab;
pub mod title_dialog;
pub mod window;

/// One row of the `class.zig` → gtk4-rs porter's map. Live, test-checked data so
/// the mapping cannot silently rot as the 22 classes are ported (L5.1+): the map
/// is the acceptance surface for "did this construct get re-expressed correctly".
#[derive(Debug, Clone, Copy)]
pub struct IdiomRow {
    /// The `class.zig` construct (the `Common` member or Zig comptime helper).
    pub zig: &'static str,
    /// Its reference site, `class.zig:<line>`.
    pub reference: &'static str,
    /// The gtk4-rs idiom that replaces it.
    pub gtk4_rs: &'static str,
    /// `true` iff the construct is an **R1 waiver** — behavior-preserving but not
    /// line-preserving because it relied on raw GObject memory layout that
    /// gtk4-rs deliberately does not expose (`getClass`, `defineVirtualMethod`).
    pub r1_waiver: bool,
}

/// The porter's map from every `class.zig::Common` construct (and the Zig
/// comptime helpers it leans on) to its gtk4-rs replacement. This is the L5.1+
/// class ports' acceptance surface — see the module doc for the rendered table.
//
// PORT NOTE: each row below is preceded by the reference `Common` member it
// replaces, with that member's reference comments kept verbatim (R1): the row
// is the Rust site of the construct (the gtk4-rs macros subsume the code).
pub const IDIOM_MAP: &[IdiomRow] = &[
    // /// Common methods for all GObject classes we create.
    // pub fn Common(comptime Self: type, comptime Private: ?type) type
    IdiomRow {
        zig: "Common(Self, Private)",
        reference: "class.zig:16",
        gtk4_rs: "#[glib::object_subclass] on the ObjectSubclass impl; Private -> its instance struct",
        r1_waiver: false,
    },
    // /// Upcast our type to a parent type or interface. This will fail at
    // /// compile time if the cast isn't 100% safe. For unsafe casts,
    // /// use `gobject.ext.cast` instead. We don't have a helper for that
    // /// because its uncommon and unsafe behavior should be noisier.
    // pub fn as(self: *Self, comptime T: type) *T
    IdiomRow {
        zig: "as(self, T)",
        reference: "class.zig:25",
        gtk4_rs: ".upcast()/.upcast_ref() via IsA<T> (safe widening)",
        r1_waiver: false,
    },
    IdiomRow {
        zig: "gobject.ext.cast (unsafe cast)",
        reference: "class.zig:23",
        gtk4_rs: ".dynamic_cast()/.downcast() (the noisier unsafe path)",
        r1_waiver: false,
    },
    // /// Increase the reference count of the object.
    // pub fn ref(self: *Self) *Self
    //
    // /// If the reference count is 1 and the object is floating, clear the
    // /// floating attribute. Otherwise, increase the reference count by 1.
    // pub fn refSink(self: *Self) *Self
    //
    // /// Decrease the reference count of the object.
    // pub fn unref(self: *Self) void
    IdiomRow {
        zig: "ref/refSink/unref",
        reference: "class.zig:30",
        gtk4_rs: "automatic: the gtk4-rs wrapper Clone/Drop; floating refs sunk by glib::Object::new (Invariant 12)",
        r1_waiver: false,
    },
    // /// Access the private data of the object. This should be forwarded
    // /// via a non-pub const usually.
    // pub const private = ...
    IdiomRow {
        zig: "private(self)",
        reference: "class.zig:47",
        gtk4_rs: "self.imp() (ObjectSubclassExt)",
        r1_waiver: false,
    },
    // /// Get the class for the object.
    // ///
    // /// This _seems_ ugly and unsafe but this is how GObject
    // /// works under the hood. From the [GObject Type System
    // /// Concepts](https://docs.gtk.org/gobject/concepts.html) documentation:
    // ///
    // ///     Every object must define two structures: its class structure
    // ///     and its instance structure. All class structures must contain
    // ///     as first member a GTypeClass structure. All instance structures
    // ///     must contain as first member a GTypeInstance structure.
    // ///     …
    // ///     These constraints allow the type system to make sure that
    // ///     every object instance (identified by a pointer to the object’s
    // ///     instance structure) contains in its first bytes a pointer to the
    // ///     object’s class structure.
    // ///     …
    // ///     The C standard mandates that the first field of a C structure is
    // ///     stored starting in the first byte of the buffer used to hold the
    // ///     structure’s fields in memory. This means that the first field of
    // ///     an instance of an object B is A’s first field which in turn is
    // ///     GTypeInstance‘s first field which in turn is g_class, a pointer
    // ///     to B’s class structure.
    // ///
    // /// This means that to access the class structure for an object you cast it
    // /// to `*gobject.TypeInstance` and then access the `f_g_class` field.
    // ///
    // /// https://gitlab.gnome.org/GNOME/glib/-/blob/2c08654b62d52a31c4e4d13d7d85e12b989e72be/gobject/gtype.h#L555-571
    // /// https://gitlab.gnome.org/GNOME/glib/-/blob/2c08654b62d52a31c4e4d13d7d85e12b989e72be/gobject/gtype.h#L2673
    // pub fn getClass(self: *Self) ?*Self.Class
    IdiomRow {
        zig: "getClass / f_g_class",
        reference: "class.zig:86",
        gtk4_rs: "DOES NOT PORT: raw TypeInstance layout; use the safe *Impl traits",
        r1_waiver: true,
    },
    // /// Define a virtual method. The `Self.Class` type must have a field
    // /// named `name` which is a function pointer in the following form:
    // ///
    // ///   ?*const fn (*Self) callconv(.c) void
    // ///
    // /// The virtual method may take additional parameters and specify
    // /// a non-void return type. The parameters and return type must be
    // /// valid for the C calling convention.
    // pub fn defineVirtualMethod(comptime name: [:0]const u8) type
    //
    //     /// The type info of the virtual method.
    //     const fn_info = fn_info: {
    //         // This is broken down like this so its slightly more
    //         // readable. We expect a field named "name" on the Class
    //         // with the rough type of `?*const fn` and we need the
    //         // function info.
    //     };
    //
    //     /// The instance type for a class.
    //     fn ClassInstance(comptime T: type) type
    //
    //     /// The function type for implementations. This is the same type
    //     /// as the virtual method but the self parameter points to the
    //     /// target instead of the original class.
    //     fn ImplementFunc(comptime T: type) type
    IdiomRow {
        zig: "defineVirtualMethod(name)",
        reference: "class.zig:99",
        gtk4_rs: "override the method in the subclass's *Impl trait",
        r1_waiver: true,
    },
    // /// A helper that creates a property that reads and writes a
    // /// private field with only shallow copies. This is good for primitives
    // /// such as bools, numbers, etc.
    // pub fn privateShallowFieldAccessor(comptime name: []const u8) ...
    IdiomRow {
        zig: "privateShallowFieldAccessor",
        reference: "class.zig:165",
        gtk4_rs: "#[property(get, set)] on a primitive field (shallow copy)",
        r1_waiver: false,
    },
    // /// A helper that can be used to create a property that reads and
    // /// writes a private boxed gobject field type.
    // ///
    // /// Reading the property will result in allocating a pointer and
    // /// setting it will free the previous pointer.
    // ///
    // /// The object class (Self) must still free the private field
    // /// in finalize!
    // pub fn privateBoxedFieldAccessor(comptime name: []const u8) ...
    IdiomRow {
        zig: "privateBoxedFieldAccessor",
        reference: "class.zig:187",
        gtk4_rs: "#[property] boxed field: get transfer .none, set transfer .full (frees prior)",
        r1_waiver: false,
    },
    // /// A helper that can be used to create a property that reads and
    // /// writes a private field gobject field type (reference counted).
    // ///
    // /// Reading the property will result in taking a reference to the
    // /// value and writing the property will unref the previous value.
    // ///
    // /// The object class (Self) must still free the private field
    // /// in finalize!
    // pub fn privateObjFieldAccessor(comptime name: []const u8) ...
    IdiomRow {
        zig: "privateObjFieldAccessor",
        reference: "class.zig:230",
        gtk4_rs: "#[property] GObject field: get transfer .none (refs), set transfer .full (unrefs prior)",
        r1_waiver: false,
    },
    // /// A helper that can be used to create a property that reads and
    // /// writes a private `?[:0]const u8` field type.
    // ///
    // /// Reading the property will result in a copy of the string
    // /// and callers are responsible for freeing it.
    // ///
    // /// Writing the property will free the previous value and copy
    // /// the new value into the private field.
    // ///
    // /// The object class (Self) must still free the private field
    // /// in finalize!
    // pub fn privateStringFieldAccessor(comptime name: []const u8) ...
    //
    //     fn setter(self: *Self, value: ?[:0]const u8) void {
    //         // We don't need to copy this because it was already
    //         // copied by the typedAccessor.
    //     }
    IdiomRow {
        zig: "privateStringFieldAccessor",
        reference: "class.zig:271",
        gtk4_rs: "#[property] Option<String> field: getter_transfer .none, setter_transfer .full",
        r1_waiver: false,
    },
    // /// Common class functions.
    // pub const Class = struct {
    //
    //     /// Bind a template child to a private entry in the class.
    //     pub const bindTemplateChildPrivate = ...
    IdiomRow {
        zig: "Class.bindTemplateChildPrivate",
        reference: "class.zig:313",
        gtk4_rs: "#[template_child] field in the CompositeTemplate derive",
        r1_waiver: false,
    },
    //     /// Bind a function pointer to a template callback symbol.
    //     pub fn bindTemplateCallback(class: *Self.Class, comptime name: [:0]const u8, comptime func: anytype) void
    //
    //         if (func_ti.@"fn".return_type == bool) {
    //             // glib booleans are ints and returning a Zig bool type
    //             // I think uses a byte and causes ABI issues.
    //             @compileError("bound function must return c_int instead of bool");
    //         }
    IdiomRow {
        zig: "Class.bindTemplateCallback (+ bool->c_int guard)",
        reference: "class.zig:330",
        gtk4_rs: "#[template_callback] fn; a gboolean signal's handler returns bool (TemplateCallbackReturn)",
        r1_waiver: false,
    },
];

/// The return type of a `#[template_callback]` bound to a `gboolean` signal.
///
/// The reference's guard `class.zig:348-352` ("bound function must return c_int
/// instead of bool") protects a C function pointer GTK calls directly, where a
/// one-byte Zig `bool` would not match `gboolean` (`c_int`). A
/// `#[template_callback]` is not called that way: glib-rs wraps it in a
/// `GClosure` whose marshaller converts the Rust return value to a `GValue` and
/// asserts that its type is the signal's return type (glib 0.22
/// `closure.rs:243-247`). For a `gboolean` signal that type is
/// `G_TYPE_BOOLEAN`, which only `bool` (or `glib::Propagation`) produces.
///
/// CORRECTION (task L-4, NEW-FINDINGS N336): this was `i32`, documented as "must
/// return `i32`, never Rust `bool`". An `i32` is a `G_TYPE_INT` value, so a
/// callback following that advice makes the marshaller panic inside the C
/// callback ("Closure returned a value of type gint but caller expected
/// gboolean"), which aborts the process: the UD295 failure mode. No callback
/// used the alias; every gboolean callback returns `bool` or `Propagation`.
/// MEASURED on the VM: `template_callback_return_answers_a_gboolean_signal`.
pub type TemplateCallbackReturn = bool;

/// The gboolean `TRUE` value a `#[template_callback]` returns as
/// [`TemplateCallbackReturn`].
pub const TEMPLATE_CALLBACK_RETURN: TemplateCallbackReturn = true;

#[cfg(test)]
mod tests {
    use super::*;

    /// N336: a handler returning [`TemplateCallbackReturn`] answers a real
    /// `gboolean` signal through glib-rs's closure marshaller (the path a
    /// `#[template_callback]` takes). `GApplication::name-lost` is a `gboolean`
    /// signal with no arguments that needs no display. The handler must run
    /// (`fired`) and its answer must reach the emitter; a wrongly typed return
    /// (the former `i32`) aborts the test process in the marshaller.
    #[test]
    fn template_callback_return_answers_a_gboolean_signal() {
        use gio::prelude::*;
        use std::cell::Cell;
        use std::rc::Rc;

        let app = gio::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
        let fired = Rc::new(Cell::new(0u32));
        let seen = fired.clone();
        app.connect_closure(
            "name-lost",
            false,
            glib::closure_local!(move |_: gio::Application| -> TemplateCallbackReturn {
                seen.set(seen.get() + 1);
                TEMPLATE_CALLBACK_RETURN
            }),
        );
        let handled = app.emit_by_name::<bool>("name-lost", &[]);
        assert_eq!(fired.get(), 1, "the handler ran");
        assert!(handled, "its TRUE reached the emitter");
    }

    /// PORTS: `class.zig:365` `test { refAllDecls(@This()); }` — a compile-touch
    /// over the module's public items. Here the module's public surface is the
    /// porter's map itself, so the touch also asserts the map is populated and
    /// covers the `Common` constructs the class ports depend on.
    #[test]
    fn class_ref_all_decls() {
        // compile-touch the public type aliases / consts (refAllDecls analogue)
        let _: TemplateCallbackReturn = TEMPLATE_CALLBACK_RETURN;

        assert!(!IDIOM_MAP.is_empty(), "the porter's map must be populated");
        for row in IDIOM_MAP {
            assert!(!row.zig.is_empty());
            assert!(!row.reference.is_empty());
            assert!(!row.gtk4_rs.is_empty());
        }

        // every named `Common` construct the class ports rely on must be mapped
        for needle in [
            "Common",
            "as",
            "defineVirtualMethod",
            "privateBoxedFieldAccessor",
            "privateObjFieldAccessor",
            "privateStringFieldAccessor",
            "bindTemplateChildPrivate",
            "bindTemplateCallback",
        ] {
            assert!(
                IDIOM_MAP.iter().any(|r| r.zig.contains(needle)),
                "porter's map is missing `{needle}`"
            );
        }

        // the two R1 waivers (raw-layout constructs) must be flagged as such
        assert!(
            IDIOM_MAP.iter().filter(|r| r.r1_waiver).count() >= 2,
            "getClass + defineVirtualMethod must be flagged as R1 waivers"
        );
    }
}
