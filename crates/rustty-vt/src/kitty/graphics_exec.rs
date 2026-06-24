//! rustty-vt kitty/graphics_exec
//!
//! PORTS: ghostty src/terminal/kitty/graphics_exec.zig
//!
//! PORT STATUS: stub
//!
//! PORT NOTE: DEFERRED to vt-12 (needs Screen/Terminal). This module ports
//! `graphics_exec.zig` (the `execute` entry point over `Terminal`); until vt-12
//! it is a compiling stub. The `kitty/graphics.zig` aggregator re-export of
//! `execute` is held back here until the real function exists.

/// Placeholder marker so the aggregator (`kitty/graphics.rs`) can reference this
/// module's wiring while the real `execute` is DEFERRED to vt-12.
#[doc(hidden)]
pub struct Deferred;
