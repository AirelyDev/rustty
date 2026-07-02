//! rustty-inspector
//!
//! PORTS: ghostty src/inspector/ (Inspector, main, units, widgets, widgets/*); links dcimgui-sys;
//! core via refs/trait (breaks #11, #12). Fallback A6: may merge with renderer+termio.
//!
//! PORT STATUS: partial — core-tier chunk C1 (CORE_BLUEPRINT §4 / row C1)
//! carves the minimal surface core's input hot path needs: `widgets::key`'s
//! pure-data `Event` + `EventRing` + `label()` (partial-file, [ADV-FIX 7]),
//! this file's `KeyEvent` re-export, and the uninhabited `Inspector`
//! placeholder. ALL inspector GUI logic (Inspector.zig, units.zig, the ImGui
//! widgets, dcimgui-sys linkage) stays Phase 3 (DEFERRALS.md
//! "rustty-inspector C1").

// This file ports ghostty src/inspector/main.zig:

// pub const widgets = @import("widgets.zig");
pub mod widgets;

// pub const Inspector = @import("Inspector.zig");
//
// PORT NOTE (DEFERRED SEAM, Phase 3): Inspector.zig drags the full ImGui
// widget tree (`widgets.surface.Inspector` -> key.Stream / terminal_info /
// vt_stream / renderer_info) plus dcimgui — a multi-day Phase-3 job
// (CORE_BLUEPRINT §4). Until it lands, the type is this opaque, UNINHABITED
// placeholder (exact precedent: `rustty_renderer::state::Inspector`), so it
// can never be constructed: core stores `Option<*mut rustty_inspector::
// Inspector>` (always `None` until Phase 3) and `activateInspector`/
// `deactivateInspector` — the only constructors — are verbatim-comment
// deferred in core (they `alloc.create(Inspector)`, unconstructible today).
// The single `renderer_state.inspector = ...` assignment casts this pointee
// to `*mut rustty_renderer::state::Inspector` (a documented opaque-pointer
// cast between the two uninhabited placeholders; the edge must point
// inspector -> renderer, never the reverse).
pub enum Inspector {}

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
    }
}
