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
//! `Cargo.toml`). The reference's *comptime* version branches collapse to their
//! modern arm; its *runtime* predicates (`gtk_version.runtimeAtLeast/
//! runtimeUntil`) stay live and are ported in L5.0 T2.

#[cfg(test)]
mod tests {
    #[test]
    fn gtk_links() {
        // Compile-touch: the gtk4-rs stack resolves against system pkg-config.
        assert!(gtk4::major_version() >= 4);
    }
}
