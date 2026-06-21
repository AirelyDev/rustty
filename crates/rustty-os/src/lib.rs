//! rustty-os
//!
//! PORTS: ghostty src/os/ (34 files incl. wasm/, string_encoding, shell, xdg, path, i18n, ...)
//! + pty.zig. pty.c is built via cc (break #14 relocates os/open->apprt).
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
