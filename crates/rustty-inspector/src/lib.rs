//! rustty-inspector
//!
//! PORTS: ghostty src/inspector/ (Inspector, main, units, widgets, widgets/*); links dcimgui-sys;
//! core via refs/trait (breaks #11, #12). Fallback A6: may merge with renderer+termio.
//!
//! PORT STATUS: partial — Task 17 landed the top-level aggregate. The `Inspector`
//! struct (Inspector.zig) now lives in its own `Inspector.rs` (this file ports
//! `main.zig`, which re-exports it), carrying `mouse` (LIVE) + the two core-
//! reachable sub-objects (`renderer_info` / `key_stream_events`) as direct fields
//! under the grows-in-place doctrine, with `setup` / `init` / `deinit` /
//! `rendererInfo` / `recordKeyEvent` ported LIVE and `recordPtyRead` / `render`
//! carried as preserved-verbatim reference bodies (blocked on the still-DEFERRED
//! `widgets::surface::Inspector` gui aggregate + the cyclic core `Surface`
//! accessor-trait seam). See `Inspector.rs`'s module doc for the full blocker
//! chain. All widget sub-modules are wired below.

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
        // Task 17 aggregate surface: the LIVE `Mouse` field type + the LIVE
        // `termio::Stream` the deferred `render`/`recordPtyRead` will drive via
        // `gui` (the `surface::Inspector` aggregate itself is still DEFERRED).
        let _ = std::any::type_name::<crate::widgets::surface::Mouse>();
        let _ = std::any::type_name::<crate::widgets::termio::Stream>();
    }
}
