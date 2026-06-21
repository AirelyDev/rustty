//! rustty-vt
//!
//! PORTS: ghostty src/terminal/ (all except terminal/c/) + the VT input slice
//! (input/key, key_encode, mouse, mouse_encode, paste) + stb/ shim. The crown jewel
//! (2001 ported tests, ~62% of the port). Residual font/global leaks ride `sys` hooks.
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
