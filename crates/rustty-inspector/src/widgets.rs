//! Inspector widgets module hub.
//!
//! PORTS: ghostty src/inspector/widgets.zig (module wiring only)
//!
//! PORT STATUS: stub — only the `key` sub-module is carved (core-tier chunk
//! C1, CORE_BLUEPRINT §4: the pure-data `Event`/`EventRing`/`label` surface
//! threaded through core's key-input hot path). The rest of widgets.zig —
//! the other widget sub-modules and the ImGui helpers (`helpMarker`,
//! `DetachableHeader`, `DetachableHeaderState`, `detachableHeader`) — is the
//! Phase-3 inspector GUI port over dcimgui (DEFERRALS.md "rustty-inspector
//! C1"); it fills in here, in place.

// const cimgui = @import("dcimgui");            (Phase 3 — dcimgui not wired)

// pub const page = @import("widgets/page.zig");           (Phase 3)
// pub const pagelist = @import("widgets/pagelist.zig");    (Phase 3)
// pub const key = @import("widgets/key.zig");
pub mod key;
// pub const renderer = @import("widgets/renderer.zig");    (Phase 3)
// pub const screen = @import("widgets/screen.zig");        (Phase 3)
// pub const style = @import("widgets/style.zig");          (Phase 3)
// pub const surface = @import("widgets/surface.zig");      (Phase 3)
// pub const terminal = @import("widgets/terminal.zig");    (Phase 3)
// pub const termio = @import("widgets/termio.zig");        (Phase 3)
