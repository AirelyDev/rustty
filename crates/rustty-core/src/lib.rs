//! rustty-core
//!
//! PORTS: ghostty src/App.zig, Surface.zig, surface_mouse.zig + the high-level
//! wiring split from global.zig (break #1, additive glue). Depends on apprt-api, not apprt.
//! (src/Command.zig is RELOCATED to rustty-termio — break #13b, Command-in-core
//! would cycle; Surface reaches it as `rustty_termio::Command`.)
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
