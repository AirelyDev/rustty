//! rustty-apprt
//!
//! PORTS: ghostty src/apprt/ (gtk/, embedded, none, browser, action, surface, ipc, structs,
//! runtime, ...) minus the apprt-api iface seam; links gtk-rs/gobject, objc, wayland
//! (bindings, not reimplementations, per R3). Consumes breaks #8, #10, #13, #14.
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
