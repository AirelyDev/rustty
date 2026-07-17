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
//! Do NOT read AD-3 as "the version branches are comptime, so delete them" —
//! that deletes live behavior. `gtk_version.atLeast` is NOT a comptime
//! predicate (`gtk_version.zig:49-68`; same shape in `adw_version.zig:47-65`):
//! it is an `inline fn` that folds to `false` at comptime ONLY when the build
//! headers are older than the threshold, and otherwise falls through to
//! `runtimeAtLeast`. It therefore folds only at comptime CALL SITES. Concretely,
//! for L5.0 T2:
//!
//! * Collapse to the MODERN arm: the comptime-context sites at/below the floor —
//!   the `adw` set AD-3 enumerates (`dialog.zig:21`, `debug_warning.zig:14,42`,
//!   `clipboard_confirmation_dialog.zig:17,301`, `surface_child_exited.zig:15`).
//! * Collapse to the LEGACY arm: comptime-forced sites ABOVE the floor —
//!   `application.zig:1261` (`comptime gtk_version.atLeast(4, 16, 0)`), which is
//!   false at a 4.14 baseline. (`CssSection::bytes()` is `v4_16`-gated in
//!   gtk4-rs, so the modern arm could not compile here regardless.)
//! * Stay LIVE, port as runtime predicates: `window.zig:695` (the
//!   `window-theme-ghostty` CSS-class gate), `window.zig:1181`,
//!   `window.zig:1799`, and every `runtimeAtLeast`/`runtimeUntil` site.
//! * DEFERRED (cost of the floor, ratified): `application.zig:1565`'s
//!   `prefers-color-scheme` propagation — see `.port/phase-2/DEFERRALS.md`.

#[cfg(test)]
mod tests {
    #[test]
    fn gtk_links() {
        // Compile-touch: the gtk4-rs stack resolves against system pkg-config.
        assert!(gtk4::major_version() >= 4);
    }
}
