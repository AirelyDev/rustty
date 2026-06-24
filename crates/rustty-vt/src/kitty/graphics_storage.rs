//! rustty-vt kitty/graphics_storage
//!
//! PORTS: ghostty src/terminal/kitty/graphics_storage.zig
//!
//! PORT STATUS: stub
//!
//! PORT NOTE: DEFERRED to vt-12 (needs Screen/Terminal). This module ports
//! `graphics_storage.zig` (the `ImageStorage` over `Screen`); until vt-12 it is
//! a compiling stub. The `kitty/graphics.zig` aggregator re-export of
//! `ImageStorage` is held back here until the real type exists.

/// Placeholder marker so the aggregator (`kitty/graphics.rs`) can reference this
/// module's wiring while the real `ImageStorage` is DEFERRED to vt-12.
#[doc(hidden)]
pub struct Deferred;
