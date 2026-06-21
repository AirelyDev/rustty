//! rustty-vt-c
//!
//! PORTS: ghostty src/terminal/c/ (terminal, render, kitty_graphics, selection_gesture, ...) —
//! the C ABI layer (break #4). cdylib+staticlib+rlib; cbindgen -> include/rustty.h, rustty_* symbols.
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
