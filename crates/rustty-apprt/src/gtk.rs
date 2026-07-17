//! rustty-apprt::gtk
//!
//! PORTS: ghostty `src/apprt/gtk.zig` (17 loc, 1 `test{}` refAllDecls block) —
//! the tier's re-export shim, plus (as the tree is carved) `src/apprt/gtk/**`
//! (55 `.zig` = 22,045 loc + 19 `.blp` + 4 `.css`). This is a NATIVE gtk4-rs
//! backend: a sibling of [`crate::embedded`]/[`crate::none`] that `impl`s the
//! `rustty_apprt_api` seam directly against GObject widgets — it does NOT go
//! through the libghostty C ABI (that is the macOS/Swift path). AD-1.
//!
//! PORT STATUS: scaffold (L5.0 T1). This module root is the shim's SHAPE only —
//! the `mod` tree below fills in as the tier is carved (L5 plan
//! `docs/superpowers/plans/2026-07-17-L5-gtk-apprt.md`). Nothing is re-exported
//! yet because nothing is ported yet; the honest residual is the whole tier.
//!
//! Reference (full file, `src/apprt/gtk.zig`):
//! ```zig
//! // The required comptime API for any apprt.
//! pub const App = @import("gtk/App.zig");
//! pub const Surface = @import("gtk/Surface.zig");
//! pub const resourcesDir = @import("gtk/flatpak.zig").resourcesDir;
//!
//! // The exported API, custom for the apprt.
//! pub const class = @import("gtk/class.zig");
//! pub const WeakRef = @import("gtk/weak_ref.zig").WeakRef;
//! pub const pre_exec = @import("gtk/pre_exec.zig");
//! pub const post_fork = @import("gtk/post_fork.zig");
//!
//! test {
//!     @import("std").testing.refAllDecls(@This());
//!     _ = @import("gtk/ext.zig");
//!     _ = @import("gtk/key.zig");
//!     _ = @import("gtk/portal.zig");
//! }
//! ```
//!
//! PORT NOTE (R5): the whole module is gated `#[cfg(all(target_os = "linux",
//! feature = "gtk"))]` at its `pub mod gtk;` declaration site (`lib.rs`), and
//! the gtk4-rs deps are BOTH Linux-target-gated and `gtk`-feature-optional
//! (`Cargo.toml`). The macOS/embedded build never sees a line of this tree.
//!
//! PORT NOTE (R3): `glib`/`gio`/`gdk4`/`gtk4`/`libadwaita` are gtk-rs *bindings*
//! to the SAME C libraries the reference links (`pkg-config gtk4 4.22.4`,
//! `libadwaita-1 1.9.2` on this host) — explicitly permitted by
//! `.port/PORT_RULES.md:47-58`. No `-sys` crate of our own is introduced for GTK.
//!
//! PORT NOTE (AD-3): the crate compiles against the PINNED gtk >= 4.14 /
//! adw >= 1.5 API baseline (the `gtk4/v4_14` + `libadwaita/v1_5` Cargo features,
//! `Cargo.toml`, where the full rationale lives).
//!
//! The oracle's fold is UNIVERSAL, not call-site-conditional: `atLeast` opens
//! with `if (comptime comptime_version.order(threshold) == .lt) return false;`
//! (`gtk_version.zig:58-62`), a compile-time-constant condition in an `inline
//! fn` with `comptime` params — so it collapses at EVERY call site, runtime ones
//! included ("This is inlined so that the comptime checks will disable the
//! runtime checks if the comptime checks fail", `gtk_version.zig:47-48`).
//! `comptime_version` — not the call site's context — picks the arm, and it
//! comes from the BUILD BOX's headers (`gtk_version.zig:7,14-18`, `@cInclude
//! ("gtk/gtk.h")`), i.e. 4.22 here.
//!
//! rustty has no such constant. `gtk4::minor_version()` is always a runtime read
//! of the real shared library; the `v4_14`/`v1_5` features are an API-surface
//! CEILING, not a version oracle. Hence THE RULE — every `at_least` site ports
//! LIVE, with two exceptions:
//!
//! * CEILING -> forced LEGACY arm, loss recorded: the modern arm names a symbol
//!   `v4_14` does not expose. `application.zig:1261` (`CssSection::bytes()` is
//!   `v4_16`-gated) and `application.zig:1565` (`gtk::InterfaceColorScheme` is
//!   `v4_20`-gated) are the SAME case — the `comptime`/runtime spelling of the
//!   two `if`s is irrelevant. Both are in `.port/phase-2/DEFERRALS.md`.
//! * COMPILE-TIME ARTIFACT -> collapse to the MODERN arm (at/below the floor):
//!   the branch picks a type, a file-level `const`, or the blueprint manifest,
//!   which Rust cannot defer to runtime. This is AD-3's enumerated `adw` set
//!   (`dialog.zig:21`, `debug_warning.zig:14,42`,
//!   `clipboard_confirmation_dialog.zig:17,301`, `surface_child_exited.zig:15`)
//!   and it is why the manifest is 19.
//!
//! Everything else stays LIVE: `window.zig:695` (threshold above the floor),
//! `window.zig:1181` (below the floor, but NOT in AD-3's enumerated set — port
//! it live rather than widen AD-3), and every `runtimeAtLeast`/`runtimeUntil`
//! site.
//!
//! `window.zig:1799` is AD-3's one extension BEYOND the rule: the rule would
//! keep `adw_version.supportsDialogs()` live (it is not ceiling-blocked —
//! `adw::AboutDialog` is `v1_5`-gated, i.e. available exactly at the floor), but
//! plan:101 deletes the helper and inlines the modern arm at all 6 of its call
//! sites, of which this is one. Port as ratified — the `adw` arm unconditionally,
//! dropping the `gtk.showAboutDialog` legacy arm; it is observable only on an
//! adw < 1.5 runtime, which AD-3 declares unsupported. (Shape note: the oracle's
//! `showAboutDialog` is C varargs and has no gtk-rs free-fn binding; it ports as
//! an `adw::AboutDialog` built with the same properties.) See `Cargo.toml` for
//! the full derivation and the open plan:96 reconciliation.

pub mod adw_version;
pub mod app;
pub mod build_info;
pub mod class;
pub mod ext;
pub mod gresource;
pub mod gtk_version;
pub mod surface;
pub mod weak_ref;
pub mod winproto;

// PORTS: `apprt/gtk.zig:8-9` `pub const App = @import("gtk/App.zig")` /
// `pub const Surface = @import("gtk/Surface.zig")` — the required comptime apprt
// API. In rustty these are the concrete backend types the `rustty_apprt_api` seam
// is `impl`'d against (`App` in `app.rs`, the `Surface` placeholder in
// `surface.rs`, filled by L5.1 T18).
pub use app::App;
pub use surface::Surface;
pub use weak_ref::WeakRef;

// The gresource accessor emitted by `build.rs`'s `glib-compile-resources`
// (`--c-name rustty --manual-register`): returns the process-static
// `GResource*` for the compiled `.blp` / css / icon blob. Ports the
// reference's `@cInclude("ghostty_resources.h")` `ghostty_get_resource()`
// extern (`src/apprt/gtk/class/application.zig:1868-1873`).
unsafe extern "C" {
    fn rustty_get_resource() -> *mut gio::ffi::GResource;
}

/// Register the compiled gresource blob into the process-global gio registry
/// exactly once. Ports the class-init block at
/// `src/apprt/gtk/class/application.zig:1867-1881`: it registers
/// `rustty_get_resource()` via `gio.resourcesRegister` and, on a null pointer,
/// warns and continues ("unable to load resources", `:1875-1880`) rather than
/// aborting — a missing blob degrades the UI but must not stop startup.
///
/// The reference registers inside GObject class-init (which runs once per class
/// registration); rustty guards with a [`std::sync::Once`] so Task 7's
/// class-init — and the resources KAT — can call it idempotently.
pub fn register_resources() {
    use std::sync::Once;

    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        // SAFETY: `rustty_get_resource` is the C accessor `build.rs` generates
        // and links; it returns a pointer to a process-static `GResource` (or
        // null).
        let ptr = unsafe { rustty_get_resource() };
        if ptr.is_null() {
            // Reference `class/application.zig:1875-1880`: warn and continue.
            eprintln!("rustty: unable to load resources");
            return;
        }
        // SAFETY: `ptr` is a non-null pointer to the static `GResource`.
        // `from_glib_none` borrows it (adds a ref; never frees the static),
        // matching the reference's `@ptrCast` borrow into `gio.resourcesRegister`.
        let resource: gio::Resource = unsafe { glib::translate::from_glib_none(ptr) };
        gio::resources_register(&resource);
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn gtk_links() {
        // Compile-touch: the gtk4-rs stack resolves against system pkg-config.
        assert!(gtk4::major_version() >= 4);
    }
}
