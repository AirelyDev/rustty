//! rustty-apprt::gtk::class
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
//! | `Class.bindTemplateCallback` + guard | `:330-352`  | `#[template_callback]` fn — the "return `c_int` not `bool`" ABI guard survives as **`-> i32`** (glib `gboolean` is `c_int`; see [`TEMPLATE_CALLBACK_RETURN`]) |
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
pub const IDIOM_MAP: &[IdiomRow] = &[
    IdiomRow {
        zig: "Common(Self, Private)",
        reference: "class.zig:16",
        gtk4_rs: "#[glib::object_subclass] on the ObjectSubclass impl; Private -> its instance struct",
        r1_waiver: false,
    },
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
    IdiomRow {
        zig: "ref/refSink/unref",
        reference: "class.zig:30",
        gtk4_rs: "automatic: the gtk4-rs wrapper Clone/Drop; floating refs sunk by glib::Object::new (Invariant 12)",
        r1_waiver: false,
    },
    IdiomRow {
        zig: "private(self)",
        reference: "class.zig:47",
        gtk4_rs: "self.imp() (ObjectSubclassExt)",
        r1_waiver: false,
    },
    IdiomRow {
        zig: "getClass / f_g_class",
        reference: "class.zig:86",
        gtk4_rs: "DOES NOT PORT: raw TypeInstance layout; use the safe *Impl traits",
        r1_waiver: true,
    },
    IdiomRow {
        zig: "defineVirtualMethod(name)",
        reference: "class.zig:99",
        gtk4_rs: "override the method in the subclass's *Impl trait",
        r1_waiver: true,
    },
    IdiomRow {
        zig: "privateShallowFieldAccessor",
        reference: "class.zig:165",
        gtk4_rs: "#[property(get, set)] on a primitive field (shallow copy)",
        r1_waiver: false,
    },
    IdiomRow {
        zig: "privateBoxedFieldAccessor",
        reference: "class.zig:187",
        gtk4_rs: "#[property] boxed field: get transfer .none, set transfer .full (frees prior)",
        r1_waiver: false,
    },
    IdiomRow {
        zig: "privateObjFieldAccessor",
        reference: "class.zig:230",
        gtk4_rs: "#[property] GObject field: get transfer .none (refs), set transfer .full (unrefs prior)",
        r1_waiver: false,
    },
    IdiomRow {
        zig: "privateStringFieldAccessor",
        reference: "class.zig:271",
        gtk4_rs: "#[property] Option<String> field: getter_transfer .none, setter_transfer .full",
        r1_waiver: false,
    },
    IdiomRow {
        zig: "Class.bindTemplateChildPrivate",
        reference: "class.zig:313",
        gtk4_rs: "#[template_child] field in the CompositeTemplate derive",
        r1_waiver: false,
    },
    IdiomRow {
        zig: "Class.bindTemplateCallback (+ bool->c_int guard)",
        reference: "class.zig:330",
        gtk4_rs: "#[template_callback] fn; the gboolean guard survives as -> i32 (TemplateCallbackReturn)",
        r1_waiver: false,
    },
];

/// The ABI guard `class.zig:348-352` ("bound function must return c_int instead
/// of bool") survives as this documented return type. glib's `gboolean` is
/// `c_int`; a `#[template_callback]` that GTK invokes for a gboolean-returning
/// signal must return `i32`, never Rust `bool` (which is one byte and mismatches
/// the C ABI, exactly the hazard the oracle's `@compileError` prevents).
pub type TemplateCallbackReturn = i32;

/// The gboolean `TRUE` value a `#[template_callback]` returns as
/// [`TemplateCallbackReturn`] (glib `gboolean` = `c_int`, `TRUE == 1`). Present
/// so the ABI-guard type has a live, non-`bool` witness the class ports can name.
pub const TEMPLATE_CALLBACK_RETURN: TemplateCallbackReturn = 1;

#[cfg(test)]
mod tests {
    use super::*;

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
