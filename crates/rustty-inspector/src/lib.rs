//! rustty-inspector
//!
//! PORTS: ghostty src/inspector/ (Inspector, main, units, widgets, widgets/*); links dcimgui-sys;
//! core via refs/trait (breaks #11, #12). Fallback A6: may merge with renderer+termio.
//!
//! PORT STATUS: complete — the deferred aggregation chain (`screen::Info` ->
//! `terminal::Info` -> `widgets::surface::Inspector` -> the top-level
//! `Inspector`) is fully inhabited. The `Inspector` struct (Inspector.zig) lives
//! in its own `Inspector.rs` (this file ports `main.zig`, which re-exports it),
//! carrying `mouse` (LIVE) + the whole `gui: widgets::surface::Inspector` widget
//! tree, with every method (`setup` / `init` / `deinit` / `rendererInfo` /
//! `recordKeyEvent` / `recordPtyRead` / `render`) ported LIVE. See
//! `Inspector.rs`'s module doc for the R4 acyclic Surface seam
//! (`widgets::surface::SurfaceData` / `&rustty_xev::Async`) that replaces the
//! reference's `*Surface` parameters. All widget sub-modules are wired below.

// This file ports ghostty src/inspector/main.zig:

// pub const widgets = @import("widgets.zig");
pub mod widgets;

// PORT NOTE: `units.zig` is a sibling leaf module of `main.zig`/`widgets.zig` in
// the reference (reached by the widgets via `@import("../units.zig")`, not
// re-exported by main.zig). Declared here as a crate-internal module so the
// `page`/`pagelist`/`screen` widget ports can reach `crate::units::toKibiBytes`.
pub mod units;

// pub const Inspector = @import("Inspector.zig");
//
// PORT NOTE (Task 17): the top-level `Inspector` value aggregate + its `render`
// port lives in `Inspector.rs` (the faithful 1:1 mapping of the oracle's
// `Inspector.zig`), re-exported here exactly as `main.zig`'s
// `pub const Inspector = @import("Inspector.zig")`. It carries `mouse` (LIVE) and
// the core-lifecycle-reachable `renderer_info` / `key_stream_events` fields
// (grows-in-place until the DEFERRED `widgets::surface::Inspector` gui aggregate
// inhabits and they fold under a `gui` field). Rust requires the explicit
// `#[path]` because the file is PascalCase to match the oracle.
#[path = "Inspector.rs"]
mod inspector_impl;
pub use inspector_impl::Inspector;

// pub const KeyEvent = widgets.key.Event;
pub use widgets::key::Event as KeyEvent;

// test {
//     @import("std").testing.refAllDecls(@This());
// }
#[cfg(test)]
mod tests {
    // PORT NOTE: the reference test reifies every public decl; ported as the
    // refAllDecls analogue over the crate's carved public surface (termio
    // lib.rs precedent). Grows as Phase 3 fills the crate in.
    #[test]
    fn ref_all_decls() {
        let _ = std::any::type_name::<crate::Inspector>();
        let _ = std::any::type_name::<crate::KeyEvent>();
        let _ = std::any::type_name::<crate::widgets::key::Event>();
        let _ = std::any::type_name::<crate::widgets::key::EventRing>();
        let _ = std::any::type_name::<crate::widgets::key::ImguiState>();
        let _ = std::any::type_name::<crate::widgets::key::Error>();
        let _ = std::any::type_name::<crate::widgets::renderer::Info>();
        // The aggregation chain surface: `gui`'s field type and the three
        // widget aggregates it owns/draws (screen/terminal Info + the
        // surface::Inspector/SurfaceData R4 seam itself).
        let _ = std::any::type_name::<crate::widgets::surface::Mouse>();
        let _ = std::any::type_name::<crate::widgets::surface::Inspector>();
        let _ = std::any::type_name::<crate::widgets::surface::SurfaceData>();
        let _ = std::any::type_name::<crate::widgets::terminal::Info>();
        let _ = std::any::type_name::<crate::widgets::screen::Info>();
        let _ = std::any::type_name::<crate::widgets::termio::Stream>();
    }
}
